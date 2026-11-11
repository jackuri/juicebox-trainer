//! Notices when a game in the library starts or closes, so a backup can be
//! made each time. The caller lists processes every few seconds and feeds
//! them to [`Watcher::tick`].

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum WatchKind {
    Started,
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchEvent {
    pub game_id: String,
    pub kind: WatchKind,
}

/// Which library games are running.
#[derive(Debug, Default)]
pub struct Watcher {
    running: BTreeMap<String, BTreeSet<u32>>,
}

impl Watcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// `games` are `(id, exe path)`, `processes` are `(pid, exe path)`.
    /// A game counts as started when its first process appears (including
    /// on the first tick) and as stopped when its last one is gone.
    pub fn tick(
        &mut self,
        games: &[(String, String)],
        processes: &[(u32, Option<String>)],
    ) -> Vec<WatchEvent> {
        let mut by_exe: BTreeMap<String, BTreeSet<u32>> = BTreeMap::new();
        for (pid, exe) in processes {
            if let Some(exe) = exe {
                by_exe.entry(exe.to_lowercase()).or_default().insert(*pid);
            }
        }
        let mut events = Vec::new();
        let mut next = BTreeMap::new();
        for (id, exe) in games {
            let now = by_exe.get(&exe.to_lowercase()).cloned().unwrap_or_default();
            let was = self.running.get(id).is_some_and(|p| !p.is_empty());
            match (was, now.is_empty()) {
                (false, false) => events.push(WatchEvent {
                    game_id: id.clone(),
                    kind: WatchKind::Started,
                }),
                (true, true) => events.push(WatchEvent {
                    game_id: id.clone(),
                    kind: WatchKind::Stopped,
                }),
                _ => {}
            }
            if !now.is_empty() {
                next.insert(id.clone(), now);
            }
        }
        self.running = next;
        events
    }

    pub fn is_running(&self, id: &str) -> bool {
        self.running.contains_key(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_start_and_stop_once() {
        let games = vec![("g".to_string(), r"C:\Games\G\g.exe".to_string())];
        let mut w = Watcher::new();
        let none: Vec<(u32, Option<String>)> = vec![(1, None), (2, Some(r"C:\x.exe".into()))];
        assert!(w.tick(&games, &none).is_empty());
        let one = vec![(10, Some(r"c:\games\g\G.EXE".to_string()))];
        let started = w.tick(&games, &one);
        assert_eq!(started.len(), 1);
        assert_eq!(started[0].kind, WatchKind::Started);
        assert!(w.is_running("g"));
        assert!(w.tick(&games, &one).is_empty());
        // A second copy, then the first closes: still running.
        let two = vec![(10, one[0].1.clone()), (11, one[0].1.clone())];
        assert!(w.tick(&games, &two).is_empty());
        assert!(w.tick(&games, &[(11, one[0].1.clone())]).is_empty());
        let stopped = w.tick(&games, &none);
        assert_eq!(stopped[0].kind, WatchKind::Stopped);
        assert!(!w.is_running("g"));
        // A game removed from the library is forgotten quietly.
        w.tick(&games, &one);
        assert!(w.tick(&[], &none).is_empty());
    }
}
