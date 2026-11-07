//! Save backups against the testbed game (never a real game): find its save
//! folder, back up on start and exit, compare, and restore.

use ot_saves::diff::{ChangeKind, Detail, LineTag};
use ot_saves::{
    Error, Library, Reason, RestoreOptions, Roots, Side, WatchKind, Watcher, diff, restore,
};
use testbed_harness::{TempDir, Testbed, serial};

/// `\\?\C:\...` → `C:\...` (the harness's temp paths are canonicalised).
fn plain(p: std::path::PathBuf) -> std::path::PathBuf {
    let s = p.to_string_lossy().into_owned();
    s.strip_prefix(r"\\?\").map(Into::into).unwrap_or(p)
}

fn processes() -> Vec<(u32, Option<String>)> {
    ot_core::list_processes()
        .unwrap()
        .into_iter()
        .map(|p| (p.pid, p.exe_path))
        .collect()
}

#[test]
fn back_up_compare_and_restore_testbed_saves() {
    let _s = serial();
    let tmp = TempDir::new("saves");
    let exe = plain(tmp.copy_testbed(r"Game\testbed_native.exe"));
    let low = plain(tmp.0.join("LocalLow"));
    let save_dir = low.join("TestCo").join("Testbed");
    let env = [("OT_TESTBED_SAVE_DIR", save_dir.as_path())];

    // First run: the game writes app.info, then saves.
    let mut game = Testbed::spawn_with_env(&exe, &env);
    let r = game.send("save");
    assert!(r["message"].as_str().unwrap().starts_with("saved"), "{r}");

    let roots = Roots {
        local_low: Some(low.clone()),
        ..Roots::default()
    };
    let mut lib = Library::open(tmp.0.join("store")).unwrap();
    let g = lib.add(&exe, &roots).unwrap();
    assert_eq!(g.name, "Testbed");
    assert_eq!(g.folders.len(), 1, "{:?}", g.folders);
    assert_eq!(g.folders[0].path, save_dir.to_string_lossy());
    let store = lib.store(&g.id).unwrap();
    let games = vec![(g.id.clone(), g.exe.clone())];

    let mut w = Watcher::new();
    let events = w.tick(&games, &processes());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, WatchKind::Started);
    let launch = store
        .create(&g.folder_paths(), Reason::Launch, "", Default::default())
        .unwrap()
        .snapshot;
    assert_eq!(launch.files, 1);

    // Play: earn gold and save.
    let gold_before = game.send("state")["gold"].as_i64().unwrap();
    game.send("earn 1000");
    game.send("save");

    let d = diff(
        &store,
        &g.folder_paths(),
        Side::Snapshot(&launch.id),
        Side::Current,
    )
    .unwrap();
    assert_eq!(d.files.len(), 1);
    assert_eq!(d.files[0].change, ChangeKind::Changed);
    let Some(Detail::Text { hunks, .. }) = &d.files[0].detail else {
        panic!("expected a text diff: {:?}", d.files[0].detail);
    };
    let changed: Vec<_> = hunks[0]
        .lines
        .iter()
        .filter(|l| l.tag != LineTag::Same)
        .map(|l| l.text.trim().to_string())
        .collect();
    assert_eq!(
        changed,
        vec![
            format!("\"gold\": {gold_before},"),
            format!("\"gold\": {},", gold_before + 1000)
        ]
    );

    // Restoring while the game runs is refused.
    assert!(matches!(
        restore(&g, &store, &launch.id, RestoreOptions::default()),
        Err(Error::GameRunning(_))
    ));

    // The game closes: an exit backup is made.
    game.kill();
    let events = w.tick(&games, &processes());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].kind, WatchKind::Stopped);
    let exit = store
        .create(&g.folder_paths(), Reason::Exit, "", Default::default())
        .unwrap();
    assert!(exit.new);

    // Restore the launch backup, then load it in the game.
    let report = restore(&g, &store, &launch.id, RestoreOptions::default()).unwrap();
    assert_eq!(report.written, 1);
    assert!(report.cloud_warning.is_none());
    let mut game = Testbed::spawn_with_env(&exe, &env);
    game.send("earn 5");
    let loaded = game.send("load");
    assert_eq!(loaded["message"], "loaded");
    assert_eq!(loaded["gold"].as_i64().unwrap(), gold_before);
    game.kill();

    // And undo the restore with the backup made just before it.
    restore(
        &g,
        &store,
        &report.safety_snapshot,
        RestoreOptions::default(),
    )
    .unwrap();
    let text = std::fs::read_to_string(save_dir.join("save1.json")).unwrap();
    assert!(text.contains(&format!("\"gold\": {},", gold_before + 1000)));
}

#[test]
fn edit_testbed_save_and_apply_a_quick_code() {
    let _s = serial();
    let tmp = TempDir::new("save-edit");
    let exe = plain(tmp.copy_testbed(r"Game\testbed_native.exe"));
    let low = plain(tmp.0.join("LocalLow"));
    let save_dir = low.join("TestCo").join("Testbed");
    let env = [("OT_TESTBED_SAVE_DIR", save_dir.as_path())];
    let mut game = Testbed::spawn_with_env(&exe, &env);
    game.send("save");

    let roots = Roots {
        local_low: Some(low.clone()),
        ..Roots::default()
    };
    let mut lib = Library::open(tmp.0.join("store")).unwrap();
    let g = lib.add(&exe, &roots).unwrap();
    let store = lib.store(&g.id).unwrap();
    let folder = g.folders[0].path.clone();

    let view = ot_saves::editor::open(&g, &folder, "save1.json").unwrap();
    assert_eq!(view.codec, "json");
    assert!(view.fields.iter().any(|f| f.path == "player.gold"));

    // Editing is refused while the game runs.
    let edit = ot_saves::Edit::Set {
        path: "player.health".into(),
        value: "42".into(),
    };
    assert!(matches!(
        ot_saves::editor::save(
            &g,
            &store,
            &folder,
            "save1.json",
            std::slice::from_ref(&edit)
        ),
        Err(Error::GameRunning(_))
    ));
    game.kill();

    let r = ot_saves::editor::save(&g, &store, &folder, "save1.json", &[edit]).unwrap();
    assert!(r.changed);
    let r = ot_saves::quick::apply(
        &g,
        &store,
        &roots,
        "json",
        "{localappdatalow}/TestCo/Testbed/save1.json",
        "player.gold",
        &serde_json::json!(999_999),
    )
    .unwrap();
    assert!(r.changed);
    assert_eq!(store.list().unwrap().len(), 2, "a backup before each edit");

    let mut game = Testbed::spawn_with_env(&exe, &env);
    let loaded = game.send("load");
    assert_eq!(loaded["message"], "loaded");
    assert_eq!(loaded["gold"].as_i64().unwrap(), 999_999);
    assert_eq!(loaded["health"].as_i64().unwrap(), 42);
}
