//! Saves kept in the Windows registry, mainly Unity **PlayerPrefs**
//! (`HKCU\Software\<company>\<product>`). Only keys under
//! `HKEY_CURRENT_USER\Software` are touched.
//!
//! A registry key is treated like a save folder: each value is one "file"
//! whose bytes are its type (4 bytes, little-endian) followed by its data, so
//! backups, compare and restore work the same as for files.
//!
//! Unity names values `<key>_h<hash>`; ints are `REG_DWORD`, floats
//! `REG_QWORD` (a double) and strings `REG_BINARY` (UTF-8 ending in a zero
//! byte). Edits keep each value's registry type.

use std::io;

use windows::Win32::Foundation::{ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, WIN32_ERROR};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_BINARY, REG_DWORD, REG_QWORD, REG_SZ,
    REG_VALUE_TYPE, RegCloseKey, RegDeleteValueW, RegEnumValueW, RegOpenKeyExW, RegQueryInfoKeyW,
    RegSetValueExW,
};
use windows::core::{HSTRING, PCWSTR, PWSTR};

use crate::codec::{Field, FieldKind};

const PREFIX: &str = "HKCU\\";

/// `HKCU\Software\...`?
pub fn is_registry(path: &str) -> bool {
    path.get(..PREFIX.len())
        .is_some_and(|p| p.eq_ignore_ascii_case(PREFIX))
}

/// The part after `HKCU\`, if it is under `Software\`.
fn subkey(path: &str) -> io::Result<&str> {
    let sub = path
        .get(PREFIX.len()..)
        .filter(|_| is_registry(path))
        .ok_or_else(|| io::Error::other("not an HKCU registry key"))?;
    let ok = sub.len() > "Software\\".len()
        && sub[.."Software\\".len()].eq_ignore_ascii_case("Software\\")
        && !sub.contains("\\\\")
        && !sub.ends_with('\\');
    if ok {
        Ok(sub)
    } else {
        Err(io::Error::other(
            "only keys under HKCU\\Software can be used",
        ))
    }
}

fn check(e: WIN32_ERROR) -> io::Result<()> {
    if e == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(e.0 as i32))
    }
}

struct Key(HKEY);

impl Drop for Key {
    fn drop(&mut self) {
        // SAFETY: the key was opened by RegOpenKeyExW and is closed once.
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open(path: &str, write: bool) -> io::Result<Key> {
    let sub = HSTRING::from(subkey(path)?);
    let mut key = HKEY::default();
    let access = if write {
        KEY_READ | KEY_WRITE
    } else {
        KEY_READ
    };
    // SAFETY: `sub` is a NUL-terminated wide string; `key` receives the handle.
    check(unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(sub.as_ptr()),
            None,
            access,
            &mut key,
        )
    })?;
    Ok(Key(key))
}

pub fn key_exists(path: &str) -> bool {
    open(path, false).is_ok()
}

/// One registry value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegValue {
    pub name: String,
    pub ty: u32,
    pub data: Vec<u8>,
}

impl RegValue {
    /// Type (4 bytes LE) + data: how a value is stored in a backup.
    pub fn to_bytes(&self) -> Vec<u8> {
        [&self.ty.to_le_bytes()[..], &self.data].concat()
    }

    pub fn from_bytes(name: &str, b: &[u8]) -> Option<Self> {
        let (ty, data) = b.split_at_checked(4)?;
        Some(Self {
            name: name.to_string(),
            ty: u32::from_le_bytes([ty[0], ty[1], ty[2], ty[3]]),
            data: data.to_vec(),
        })
    }
}

/// Every value of the key, sorted by name.
pub fn read_values(path: &str) -> io::Result<Vec<RegValue>> {
    let key = open(path, false)?;
    let (mut max_name, mut max_data) = (0u32, 0u32);
    // SAFETY: `key` is open; only the two out-pointers are used.
    check(unsafe {
        RegQueryInfoKeyW(
            key.0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(&mut max_name),
            Some(&mut max_data),
            None,
            None,
        )
    })?;
    let mut name_buf = vec![0u16; max_name as usize + 1];
    let mut data_buf = vec![0u8; max_data as usize + 1];
    let mut out = Vec::new();
    for index in 0.. {
        let mut name_len = name_buf.len() as u32;
        let mut data_len = data_buf.len() as u32;
        let mut ty = 0u32;
        // SAFETY: the buffers are sized from RegQueryInfoKeyW and their
        // lengths passed in; Windows writes at most that much.
        let e = unsafe {
            RegEnumValueW(
                key.0,
                index,
                Some(PWSTR(name_buf.as_mut_ptr())),
                &mut name_len,
                None,
                Some(&mut ty),
                Some(data_buf.as_mut_ptr()),
                Some(&mut data_len),
            )
        };
        if e == ERROR_NO_MORE_ITEMS {
            break;
        }
        check(e)?;
        out.push(RegValue {
            name: String::from_utf16_lossy(&name_buf[..name_len as usize]),
            ty,
            data: data_buf[..data_len as usize].to_vec(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

pub fn set_value(path: &str, v: &RegValue) -> io::Result<()> {
    let key = open(path, true)?;
    let name = HSTRING::from(v.name.as_str());
    // SAFETY: `key` is open for writing; `name` is NUL-terminated.
    check(unsafe {
        RegSetValueExW(
            key.0,
            PCWSTR(name.as_ptr()),
            None,
            REG_VALUE_TYPE(v.ty),
            Some(&v.data),
        )
    })
}

pub fn delete_value(path: &str, name: &str) -> io::Result<()> {
    let key = open(path, true)?;
    let name = HSTRING::from(name);
    // SAFETY: `key` is open for writing; `name` is NUL-terminated.
    check(unsafe { RegDeleteValueW(key.0, PCWSTR(name.as_ptr())) })
}

/// `gold_h12345` → `gold`.
pub fn display_name(name: &str) -> &str {
    if let Some(at) = name.rfind("_h") {
        let digits = &name[at + 2..];
        if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
            return &name[..at];
        }
    }
    name
}

fn text_of(data: &[u8]) -> Option<String> {
    let body = data.strip_suffix(&[0]).unwrap_or(data);
    std::str::from_utf8(body).ok().map(str::to_string)
}

fn utf16_of(data: &[u8]) -> String {
    let w: Vec<u16> = data
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    String::from_utf16_lossy(&w)
        .trim_end_matches('\0')
        .to_string()
}

/// The values as editor fields. Paths are display names (or the full value
/// name when two values would share one).
pub fn fields(values: &[RegValue]) -> Vec<Field> {
    values
        .iter()
        .map(|v| {
            let short = display_name(&v.name);
            let shared = values
                .iter()
                .filter(|o| display_name(&o.name) == short)
                .count()
                > 1;
            let path = if shared {
                v.name.clone()
            } else {
                short.to_string()
            };
            let (kind, value, editable, note) = match REG_VALUE_TYPE(v.ty) {
                REG_DWORD if v.data.len() == 4 => (
                    FieldKind::Int,
                    i32::from_le_bytes([v.data[0], v.data[1], v.data[2], v.data[3]]).to_string(),
                    true,
                    "whole number (REG_DWORD)",
                ),
                REG_QWORD if v.data.len() == 8 => {
                    let mut b = [0u8; 8];
                    b.copy_from_slice(&v.data);
                    (
                        FieldKind::Float,
                        f64::from_le_bytes(b).to_string(),
                        true,
                        "number (REG_QWORD)",
                    )
                }
                REG_BINARY => match text_of(&v.data) {
                    Some(t) => (FieldKind::Text, t, true, "text (REG_BINARY)"),
                    None => (
                        FieldKind::Other,
                        format!("{} bytes", v.data.len()),
                        false,
                        "REG_BINARY",
                    ),
                },
                REG_SZ => (FieldKind::Text, utf16_of(&v.data), true, "text (REG_SZ)"),
                _ => (
                    FieldKind::Other,
                    format!("{} bytes", v.data.len()),
                    false,
                    "other registry type",
                ),
            };
            Field {
                path,
                kind,
                value,
                editable,
                note: Some(note.into()),
            }
        })
        .collect()
}

/// The new value for the field at `path`, keeping its registry type.
pub fn edit(values: &[RegValue], path: &str, value: &str) -> Result<RegValue, String> {
    let all = fields(values);
    let i = all
        .iter()
        .position(|f| f.path == path)
        .ok_or_else(|| format!("no value named {path}"))?;
    if !all[i].editable {
        return Err(format!("{path} can't be edited"));
    }
    let old = &values[i];
    let v = value.trim();
    let data = match REG_VALUE_TYPE(old.ty) {
        REG_DWORD => v
            .parse::<i32>()
            .map_err(|_| format!("{value:?} isn't a whole number"))?
            .to_le_bytes()
            .to_vec(),
        REG_QWORD => {
            let f = v
                .parse::<f64>()
                .map_err(|_| format!("{value:?} isn't a number"))?;
            if !f.is_finite() {
                return Err(format!("{value:?} isn't a number"));
            }
            f.to_le_bytes().to_vec()
        }
        REG_BINARY => [value.as_bytes(), &[0]].concat(),
        REG_SZ => value
            .encode_utf16()
            .chain([0])
            .flat_map(u16::to_le_bytes)
            .collect(),
        _ => return Err(format!("{path} can't be edited")),
    };
    Ok(RegValue {
        name: old.name.clone(),
        ty: old.ty,
        data,
    })
}

/// A scratch key under `HKCU\Software\JuiceBoxTrainer-Tests`, deleted on drop.
#[cfg(test)]
pub(crate) struct TestKey(pub String);

#[cfg(test)]
impl TestKey {
    pub fn new(name: &str) -> Self {
        use windows::Win32::System::Registry::{REG_OPTION_NON_VOLATILE, RegCreateKeyExW};
        let path = format!(
            "HKCU\\Software\\JuiceBoxTrainer-Tests\\{}-{name}",
            std::process::id()
        );
        let sub = HSTRING::from(&path[PREFIX.len()..]);
        let mut key = HKEY::default();
        // SAFETY: test helper; creates a scratch key under HKCU.
        let e = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(sub.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_READ | KEY_WRITE,
                None,
                &mut key,
                None,
            )
        };
        check(e).unwrap();
        drop(Key(key));
        Self(path)
    }

    pub fn set(&self, name: &str, ty: REG_VALUE_TYPE, data: &[u8]) {
        set_value(
            &self.0,
            &RegValue {
                name: name.into(),
                ty: ty.0,
                data: data.to_vec(),
            },
        )
        .unwrap();
    }

    /// A Unity-style set of PlayerPrefs.
    pub fn unity_sample(&self) {
        self.set("gold_h193407567", REG_DWORD, &250i32.to_le_bytes());
        self.set("volume_h2087423", REG_QWORD, &0.75f64.to_le_bytes());
        self.set("playerName_h11", REG_BINARY, b"Tester\0");
        self.set(
            "unity.cloud_userid_h1",
            REG_BINARY,
            &[0xFF, 0xFE, 0x00, 0x01],
        );
    }
}

#[cfg(test)]
impl Drop for TestKey {
    fn drop(&mut self) {
        use windows::Win32::System::Registry::RegDeleteTreeW;
        let sub = HSTRING::from(&self.0[PREFIX.len()..]);
        // SAFETY: test helper; deletes only the scratch key it created.
        unsafe {
            let _ = RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(sub.as_ptr()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_hkcu_software_keys() {
        assert!(is_registry(r"HKCU\Software\TestCo\Testbed"));
        assert!(!is_registry(r"C:\Saves"));
        assert!(subkey(r"HKCU\Software\TestCo").is_ok());
        assert!(subkey(r"HKCU\System\CurrentControlSet").is_err());
        assert!(subkey(r"HKCU\Software\").is_err());
        assert!(subkey(r"HKLM\Software\x").is_err());
        assert!(!key_exists(
            r"HKCU\Software\JuiceBoxTrainer-Tests\does-not-exist"
        ));
    }

    #[test]
    fn reads_and_edits_player_prefs() {
        let k = TestKey::new("prefs");
        k.unity_sample();
        let values = read_values(&k.0).unwrap();
        assert_eq!(values.len(), 4);
        let f = fields(&values);
        let get = |p: &str| f.iter().find(|x| x.path == p).unwrap().clone();
        assert_eq!(get("gold").value, "250");
        assert_eq!(get("volume").kind, FieldKind::Float);
        assert_eq!(get("volume").value, "0.75");
        assert_eq!(get("playerName").value, "Tester");
        assert!(!get("unity.cloud_userid").editable);

        let new = edit(&values, "gold", "99999").unwrap();
        set_value(&k.0, &new).unwrap();
        let new = edit(&values, "playerName", "Hero").unwrap();
        set_value(&k.0, &new).unwrap();
        let after = fields(&read_values(&k.0).unwrap());
        assert!(after.iter().any(|x| x.path == "gold" && x.value == "99999"));
        assert!(
            after
                .iter()
                .any(|x| x.path == "playerName" && x.value == "Hero")
        );
        assert!(edit(&values, "gold", "lots").is_err());
        assert!(edit(&values, "unity.cloud_userid", "x").is_err());

        let first = &values[0];
        assert_eq!(
            RegValue::from_bytes(&first.name, &first.to_bytes()).as_ref(),
            Some(first)
        );
        delete_value(&k.0, &first.name).unwrap();
        assert_eq!(read_values(&k.0).unwrap().len(), 3);
    }

    #[test]
    fn display_names() {
        assert_eq!(display_name("gold_h193407567"), "gold");
        assert_eq!(display_name("no_hash"), "no_hash");
        assert_eq!(display_name("x_h"), "x_h");
    }
}
