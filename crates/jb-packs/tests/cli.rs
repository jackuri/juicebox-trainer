//! The `juicepack` command: export, inspect and index a pack.

use std::path::PathBuf;
use std::process::Command;

fn otpack(args: &[&std::ffi::OsStr]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_juicepack"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn exports_inspects_and_indexes() {
    let root = std::env::temp_dir().join(format!("ot-otpack-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let packs = root.join("packs");
    let example = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs/examples/testbed-native/pack.json");
    std::fs::create_dir_all(packs.join("testbed-native")).unwrap();
    let text = std::fs::read_to_string(&example).unwrap();
    std::fs::write(packs.join("testbed-native").join("pack.json"), &text).unwrap();

    let file = root.join("t.otpack");
    let (ok, out) = otpack(&["export".as_ref(), example.as_os_str(), file.as_os_str()]);
    assert!(ok, "{out}");
    let (ok, out) = otpack(&["inspect".as_ref(), file.as_os_str()]);
    assert!(ok, "{out}");
    assert!(out.contains("pack testbed-native v1.0.0"), "{out}");

    let index = root.join("index.json");
    let (ok, out) = otpack(&["index".as_ref(), packs.as_os_str(), index.as_os_str()]);
    assert!(ok, "{out}");
    let (read, skipped) =
        jb_packs::community::parse_index(&std::fs::read(&index).unwrap()).unwrap();
    assert!(skipped.is_empty());
    assert_eq!(read.packs.len(), 1);
    assert_eq!(
        read.packs[0].sha256,
        jb_core::hash::sha256_bytes(text.as_bytes())
    );

    // A pack in the wrong folder fails the index.
    std::fs::create_dir_all(packs.join("wrong")).unwrap();
    std::fs::write(packs.join("wrong").join("pack.json"), &text).unwrap();
    let (ok, out) = otpack(&["index".as_ref(), packs.as_os_str(), index.as_os_str()]);
    assert!(!ok);
    assert!(out.contains("but the folder is wrong"), "{out}");

    let (ok, out) = otpack(&["inspect".as_ref(), example.as_os_str()]);
    assert!(!ok);
    assert!(out.contains("isn't an .otpack"), "{out}");
    let _ = std::fs::remove_dir_all(&root);
}
