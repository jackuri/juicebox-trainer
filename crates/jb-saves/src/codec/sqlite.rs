//! SQLite saves: a database file whose tables hold the game's data. Every
//! cell of every table is a value, `table[rowid].column`
//! (`Player[1].Gold`, `Inventory[12].Count`). An edit is an `UPDATE` of that
//! one cell, then the database is written back.
//!
//! The database is opened in memory from the file's bytes (the file itself is
//! never opened by SQLite), in SQLite's defensive mode with schema code
//! untrusted and no extensions, so a save can't make SQLite do anything but
//! read and change its own tables. Text and numbers can be edited; blobs are
//! shown (size and first bytes) but not changed.

use rusqlite::config::DbConfig;
use rusqlite::types::{Value, ValueRef};
use rusqlite::{Connection, MAIN_DB, OpenFlags};

use super::json::join_key;
use super::{Field, FieldKind, MAX_FIELDS};

/// SQLite's file header.
pub(crate) const MAGIC: &[u8] = b"SQLite format 3\0";
/// Rows read per table at most.
const MAX_ROWS: usize = 20_000;

pub(crate) fn is_sqlite(b: &[u8]) -> bool {
    b.len() >= 100 && b.starts_with(MAGIC)
}

/// One cell.
#[derive(Debug, Clone)]
pub(crate) struct Cell {
    pub field: Field,
    table: String,
    rowid: i64,
    column: String,
}

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// WAL-mode files (header bytes 18 and 19 = 2) are opened as rollback
/// files, which in-memory databases need, and marked WAL again on saving.
fn wal_mode(b: &[u8]) -> bool {
    b.get(18) == Some(&2) || b.get(19) == Some(&2)
}

fn open_db(b: &[u8], read_only: bool) -> Result<Connection, String> {
    let mut image = b.to_vec();
    if wal_mode(&image) {
        image[18] = 1;
        image[19] = 1;
    }
    let mut conn = Connection::open_with_flags(
        ":memory:",
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| e.to_string())?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)
        .map_err(|e| e.to_string())?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)
        .map_err(|e| e.to_string())?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_TRIGGER, false)
        .map_err(|e| e.to_string())?;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_VIEW, false)
        .map_err(|e| e.to_string())?;
    let len = image.len();
    conn.deserialize_read_exact(MAIN_DB, image.as_slice(), len, read_only)
        .map_err(|e| format!("not a database SQLite can read ({e})"))?;
    conn.execute_batch("PRAGMA cell_size_check = ON;")
        .map_err(|e| e.to_string())?;
    let check: String = conn
        .query_row("PRAGMA quick_check(1)", [], |r| r.get(0))
        .map_err(|e| format!("the database is damaged ({e})"))?;
    if check != "ok" {
        return Err(format!("the database is damaged ({check})"));
    }
    Ok(conn)
}

fn kind_and_text(v: ValueRef<'_>) -> (FieldKind, String, bool) {
    match v {
        ValueRef::Null => (FieldKind::Null, "null".into(), true),
        ValueRef::Integer(n) => (FieldKind::Int, n.to_string(), true),
        ValueRef::Real(f) => (FieldKind::Float, format!("{f:?}"), true),
        ValueRef::Text(t) => (
            FieldKind::Text,
            String::from_utf8_lossy(t).into_owned(),
            std::str::from_utf8(t).is_ok(),
        ),
        ValueRef::Blob(b) => {
            let head: Vec<String> = b.iter().take(16).map(|x| format!("{x:02X}")).collect();
            let more = if b.len() > 16 { " …" } else { "" };
            (
                FieldKind::Other,
                format!("{} bytes: {}{more}", b.len(), head.join(" ")),
                false,
            )
        }
    }
}

/// Tables with a rowid (the ones whose cells can be found again).
fn tables(conn: &Connection) -> Result<Vec<(String, bool)>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT name, sql FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
             ORDER BY rowid",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for row in rows {
        let (name, sql) = row.map_err(|e| e.to_string())?;
        let upper = sql.to_ascii_uppercase();
        let plain = !upper.contains("WITHOUT ROWID") && !upper.starts_with("CREATE VIRTUAL");
        out.push((name, plain));
    }
    Ok(out)
}

/// Every cell, or why the file can't be read.
pub(crate) fn cells(b: &[u8]) -> Result<(Vec<Cell>, bool), String> {
    let conn = open_db(b, true)?;
    let mut out = Vec::new();
    let mut truncated = false;
    'tables: for (table, plain) in tables(&conn)? {
        if !plain {
            continue;
        }
        let sql = format!(
            "SELECT rowid, * FROM {} LIMIT {}",
            quote(&table),
            MAX_ROWS + 1
        );
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let columns: Vec<String> = stmt.column_names().skip(1).map(str::to_string).collect();
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        let mut n = 0;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            n += 1;
            if n > MAX_ROWS {
                truncated = true;
                break;
            }
            let rowid: i64 = row.get(0).map_err(|e| e.to_string())?;
            for (i, column) in columns.iter().enumerate() {
                if out.len() >= MAX_FIELDS {
                    truncated = true;
                    break 'tables;
                }
                let v = row.get_ref(i + 1).map_err(|e| e.to_string())?;
                let (kind, value, editable) = kind_and_text(v);
                let path = join_key(&format!("{}[{rowid}]", join_key("", &table)), column);
                out.push(Cell {
                    field: Field {
                        path,
                        kind,
                        value,
                        editable,
                        note: Some(format!("table {table}")),
                    },
                    table: table.clone(),
                    rowid,
                    column: column.clone(),
                });
            }
        }
    }
    Ok((out, truncated))
}

/// The fields for the editor.
pub(crate) fn parse(b: &[u8]) -> Result<(Vec<Field>, bool), String> {
    let (cells, truncated) = cells(b)?;
    Ok((cells.into_iter().map(|c| c.field).collect(), truncated))
}

fn new_value(kind: FieldKind, value: &str) -> Result<Value, String> {
    let t = value.trim();
    Ok(match kind {
        FieldKind::Int => Value::Integer(
            t.parse()
                .map_err(|_| format!("{value:?} isn't a whole number"))?,
        ),
        FieldKind::Float => {
            let f: f64 = t.parse().map_err(|_| format!("{value:?} isn't a number"))?;
            if !f.is_finite() {
                return Err(format!("{value:?} isn't a number"));
            }
            Value::Real(f)
        }
        FieldKind::Text => Value::Text(value.to_string()),
        // An empty cell takes what was typed: a number if it looks like one.
        FieldKind::Null => {
            if t.eq_ignore_ascii_case("null") {
                Value::Null
            } else if let Ok(n) = t.parse::<i64>() {
                Value::Integer(n)
            } else if let Ok(f) = t.parse::<f64>()
                && f.is_finite()
            {
                Value::Real(f)
            } else {
                Value::Text(value.to_string())
            }
        }
        FieldKind::Bool | FieldKind::Other => return Err("this value can't be edited".into()),
    })
}

/// Sets the cell at `path` and returns the new database file.
pub(crate) fn set(b: &[u8], path: &str, value: &str) -> Result<Vec<u8>, String> {
    let (cells, _) = cells(b)?;
    let cell = cells
        .iter()
        .find(|c| c.field.path == path)
        .ok_or_else(|| format!("no value at {path}"))?;
    if !cell.field.editable {
        return Err("this value can't be edited".into());
    }
    let v = new_value(cell.field.kind, value)?;
    let mut conn = open_db(b, false)?;
    let sql = format!(
        "UPDATE {} SET {} = ?1 WHERE rowid = ?2",
        quote(&cell.table),
        quote(&cell.column)
    );
    let n = conn
        .execute(&sql, rusqlite::params![v, cell.rowid])
        .map_err(|e| format!("SQLite refused the change: {e}"))?;
    if n != 1 {
        return Err(format!("no row {} in {}", cell.rowid, cell.table));
    }
    let data = conn.serialize(MAIN_DB).map_err(|e| e.to_string())?;
    let mut out = data.to_vec();
    if wal_mode(b) && out.len() > 19 {
        out[18] = b[18];
        out[19] = b[19];
    }
    Ok(out)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A small save database, made here.
    pub(crate) fn sample(wal: bool) -> Vec<u8> {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE Player (id INTEGER PRIMARY KEY, Name TEXT, Gold INTEGER, Speed REAL, Pet TEXT);
             INSERT INTO Player VALUES (1, 'Hero', 250, 1.5, NULL);
             CREATE TABLE \"odd \"\"name\" (\"col.x\" INTEGER, Icon BLOB);
             INSERT INTO \"odd \"\"name\" VALUES (7, x'89504E47');
             CREATE TABLE Kv (k TEXT PRIMARY KEY, v TEXT) WITHOUT ROWID;
             INSERT INTO Kv VALUES ('volume', '0.8');
             CREATE TRIGGER no_rich AFTER UPDATE ON Player BEGIN UPDATE Player SET Gold = 0; END;",
        )
        .unwrap();
        let mut b = conn.serialize(MAIN_DB).unwrap().to_vec();
        if wal {
            b[18] = 2;
            b[19] = 2;
        }
        b
    }

    fn get(b: &[u8], path: &str) -> Field {
        parse(b)
            .unwrap()
            .0
            .into_iter()
            .find(|f| f.path == path)
            .unwrap_or_else(|| panic!("{path}"))
    }

    #[test]
    fn lists_every_cell() {
        let db = sample(false);
        assert!(is_sqlite(&db));
        assert_eq!(get(&db, "Player[1].Gold").value, "250");
        assert_eq!(get(&db, "Player[1].Gold").kind, FieldKind::Int);
        assert_eq!(get(&db, "Player[1].Name").value, "Hero");
        assert_eq!(get(&db, "Player[1].Speed").kind, FieldKind::Float);
        assert_eq!(get(&db, "Player[1].Pet").kind, FieldKind::Null);
        let odd = get(&db, "[\"odd \\\"name\"][1][\"col.x\"]");
        assert_eq!(odd.value, "7");
        let icon = get(&db, "[\"odd \\\"name\"][1].Icon");
        assert!(!icon.editable && icon.value.starts_with("4 bytes: 89 50 4E 47"));
        // WITHOUT ROWID tables aren't listed (their rows can't be found by rowid).
        assert!(
            parse(&db)
                .unwrap()
                .0
                .iter()
                .all(|f| !f.path.starts_with("Kv"))
        );
    }

    #[test]
    fn edits_one_cell_and_triggers_do_not_run() {
        let db = sample(false);
        let out = set(&db, "Player[1].Gold", "99999").unwrap();
        assert_eq!(
            get(&out, "Player[1].Gold").value,
            "99999",
            "the trigger didn't run"
        );
        assert_eq!(get(&out, "Player[1].Name").value, "Hero");
        let out = set(&out, "Player[1].Name", "Max \"the\" Hero").unwrap();
        assert_eq!(get(&out, "Player[1].Name").value, "Max \"the\" Hero");
        // An empty cell takes the value; a TEXT column keeps it as text.
        let out = set(&out, "Player[1].Pet", "3").unwrap();
        let pet = get(&out, "Player[1].Pet");
        assert_eq!((pet.kind, pet.value.as_str()), (FieldKind::Text, "3"));
        let out = set(&out, "[\"odd \\\"name\"][1][\"col.x\"]", "8").unwrap();
        assert_eq!(get(&out, "[\"odd \\\"name\"][1][\"col.x\"]").value, "8");
        assert!(set(&db, "Player[1].Gold", "lots").is_err());
        assert!(set(&db, "Player[1].Speed", "fast").is_err());
        assert!(set(&db, "[\"odd \\\"name\"][1].Icon", "00").is_err());
        assert!(set(&db, "Player[2].Gold", "1").is_err());
    }

    #[test]
    fn keeps_wal_mode() {
        let db = sample(true);
        assert_eq!(get(&db, "Player[1].Gold").value, "250");
        let out = set(&db, "Player[1].Gold", "5").unwrap();
        assert_eq!((out[18], out[19]), (2, 2));
        assert_eq!(get(&out, "Player[1].Gold").value, "5");
    }

    #[test]
    fn damaged_files_are_refused() {
        let mut db = sample(false);
        assert!(!is_sqlite(&db[..50]));
        let half = db.len() / 2;
        db.truncate(half);
        assert!(parse(&db).is_err());
        let mut bad = sample(false);
        for b in bad.iter_mut().skip(100) {
            *b = 0xAB;
        }
        assert!(parse(&bad).is_err());
    }
}
