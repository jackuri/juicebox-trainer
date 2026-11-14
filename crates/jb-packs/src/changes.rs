//! What changed between the cheats someone shared and the ones they have now (v1.38.0), so
//! updating a shared pack or web game shows "2 new, 1 changed, 1 removed" in plain words.

use serde::Serialize;

use crate::Pack;
use crate::web::WebPack;

/// Cheats added, changed, renamed and removed, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Changes {
    pub added: Vec<String>,
    /// Same cheat, something about it changed (where it is, its value, its key).
    pub changed: Vec<String>,
    /// `(old name, new name)`, otherwise the same.
    pub renamed: Vec<(String, String)>,
    pub removed: Vec<String>,
    /// The notes or who found them changed.
    pub details: bool,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.changed.is_empty()
            && self.renamed.is_empty()
            && self.removed.is_empty()
            && !self.details
    }
}

/// Pairs old and new items: first by `same` (the same cheat), then the rest by name.
/// Returns the changes, using `name` for each item and `equal` to tell if a pair is unchanged.
fn compare<T>(
    old: &[T],
    new: &[T],
    name: impl Fn(&T) -> &str,
    same: impl Fn(&T, &T) -> bool,
    equal: impl Fn(&T, &T) -> bool,
) -> Changes {
    let mut out = Changes::default();
    let mut old_used = vec![false; old.len()];
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut unmatched = Vec::new();
    for (j, n) in new.iter().enumerate() {
        match (0..old.len()).find(|&i| !old_used[i] && same(&old[i], n)) {
            Some(i) => {
                old_used[i] = true;
                pairs.push((i, j));
            }
            None => unmatched.push(j),
        }
    }
    // Same name but moved (a fixed address): changed, not added and removed.
    for j in unmatched {
        match (0..old.len()).find(|&i| !old_used[i] && name(&old[i]) == name(&new[j])) {
            Some(i) => {
                old_used[i] = true;
                pairs.push((i, j));
            }
            None => out.added.push(name(&new[j]).to_string()),
        }
    }
    pairs.sort_by_key(|&(_, j)| j);
    for (i, j) in pairs {
        let (o, n) = (&old[i], &new[j]);
        if name(o) != name(n) {
            out.renamed.push((name(o).to_string(), name(n).to_string()));
        }
        if !equal(o, n) {
            out.changed.push(name(n).to_string());
        }
    }
    out.removed = old
        .iter()
        .zip(&old_used)
        .filter(|(_, used)| !**used)
        .map(|(o, _)| name(o).to_string())
        .collect();
    out
}

/// Web cheats: the same cheat is the same place in the game.
pub fn web(old: &WebPack, new: &WebPack) -> Changes {
    let mut c = compare(
        &old.cheats,
        &new.cheats,
        |c| c.name.as_str(),
        |a, b| a.target == b.target,
        |a, b| {
            a.target == b.target
                && a.value == b.value
                && a.key == b.key
                && a.key_does == b.key_does
                && a.keep == b.keep
        },
    );
    c.details = old.notes != new.notes || old.by != new.by;
    c
}

/// A pack's cheats: the same cheat has the same id.
pub fn pack(old: &Pack, new: &Pack) -> Changes {
    let json = |c: &crate::Cheat| {
        let mut v = serde_json::to_value(c).unwrap_or_default();
        if let Some(o) = v.as_object_mut() {
            o.remove("name");
        }
        v
    };
    let mut c = compare(
        &old.cheats,
        &new.cheats,
        |c| c.name.as_str(),
        |a, b| a.id == b.id,
        |a, b| json(a) == json(b),
    );
    c.details = old.meta.notes != new.meta.notes || old.meta.author != new.meta.author;
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web_pack(cheats: serde_json::Value, notes: &str) -> WebPack {
        serde_json::from_value(serde_json::json!({
            "format": 1, "id": "outbreak-07307fbbf2", "game": "Outbreak",
            "url": "https://www.outbreakweb.com/", "by": "Jack", "notes": notes,
            "cheats": cheats,
        }))
        .unwrap()
    }

    fn target(off: u64) -> serde_json::Value {
        serde_json::json!({ "k": "m", "mem": 0, "label": "memory", "size": 322174976, "off": off, "t": "i32" })
    }

    #[test]
    fn web_cheats_by_place_then_name() {
        let old = web_pack(
            serde_json::json!([
                { "name": "Infinite health (Kevin)", "target": target(100), "value": 2300, "key": "F5", "keyDoes": "lock" },
                { "name": "Infinite pistol ammo", "target": target(200), "value": 99 },
                { "name": "Weak enemy 1 (1 HP)", "target": target(300), "value": 1 },
                { "name": "Old cheat", "target": target(400), "value": 5 },
            ]),
            "lock to 99",
        );
        let new = web_pack(
            serde_json::json!([
                // Renamed and a new value.
                { "name": "Infinite health (my character, any)", "target": target(100), "value": 3000, "key": "F5", "keyDoes": "lock" },
                // Unchanged.
                { "name": "Infinite pistol ammo", "target": target(200), "value": 99 },
                // Same name, fixed address.
                { "name": "Weak enemy 1 (1 HP)", "target": target(310), "value": 1 },
                // New.
                { "name": "No virus (whole team)", "target": target(500), "value": 0 },
            ]),
            "lock to 99",
        );
        let c = web(&old, &new);
        assert_eq!(c.added, ["No virus (whole team)"]);
        assert_eq!(
            c.renamed,
            [(
                "Infinite health (Kevin)".to_string(),
                "Infinite health (my character, any)".to_string()
            )]
        );
        assert_eq!(
            c.changed,
            ["Infinite health (my character, any)", "Weak enemy 1 (1 HP)"]
        );
        assert_eq!(c.removed, ["Old cheat"]);
        assert!(!c.details);
        assert!(!c.is_empty());
        assert!(web(&old, &old).is_empty());
        let mut noted = old.clone();
        noted.notes = Some("new notes".into());
        assert!(web(&old, &noted).details);
    }

    #[test]
    fn pack_cheats_by_id() {
        let text = include_str!("../../../packs/examples/testbed-native/pack.json");
        let (old, _) = crate::parse(text).unwrap();
        let mut new = old.clone();
        assert!(pack(&old, &new).is_empty());
        new.cheats[0].name = "Never die".into();
        new.cheats[1].hotkey = Some("F9".into());
        let gone = new.cheats.remove(2);
        let mut extra = new.cheats[0].clone();
        extra.id = "extra".into();
        extra.name = "Extra".into();
        new.cheats.push(extra);
        let c = pack(&old, &new);
        assert_eq!(
            c.renamed,
            [(old.cheats[0].name.clone(), "Never die".to_string())]
        );
        assert_eq!(c.changed, [new.cheats[1].name.clone()]);
        assert_eq!(c.removed, [gone.name]);
        assert_eq!(c.added, ["Extra"]);
    }
}
