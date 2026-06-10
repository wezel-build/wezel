//! `Snapshot` must restore the project tree byte- and mtime-identically:
//! sampled iterations are only i.i.d. if every sample starts from the exact
//! captured state, and cargo-style staleness checks depend on mtimes.

use std::fs;
use std::time::{Duration, SystemTime};

use wezel_bench::workspace::Snapshot;

#[test]
fn restore_returns_tree_to_captured_state() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("kept.txt"), "original").unwrap();
    fs::write(root.join("sub/artifact.bin"), "blob").unwrap();

    // Age the files so preserved mtimes are distinguishable from fresh ones.
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    for path in ["kept.txt", "sub/artifact.bin"] {
        let file = fs::File::options()
            .write(true)
            .open(root.join(path))
            .unwrap();
        file.set_modified(old).unwrap();
    }
    let kept_mtime = fs::metadata(root.join("kept.txt"))
        .unwrap()
        .modified()
        .unwrap();
    let artifact_mtime = fs::metadata(root.join("sub/artifact.bin"))
        .unwrap()
        .modified()
        .unwrap();

    let snapshot = Snapshot::capture(root).unwrap();

    // Mutate the tree in three ways: modify, delete, add.
    fs::write(root.join("kept.txt"), "scribbled").unwrap();
    fs::remove_file(root.join("sub/artifact.bin")).unwrap();
    fs::write(root.join("new.txt"), "junk").unwrap();

    snapshot.restore_to(root).unwrap();

    assert_eq!(
        fs::read_to_string(root.join("kept.txt")).unwrap(),
        "original"
    );
    assert_eq!(fs::read(root.join("sub/artifact.bin")).unwrap(), b"blob");
    assert!(!root.join("new.txt").exists());
    assert_eq!(
        fs::metadata(root.join("kept.txt"))
            .unwrap()
            .modified()
            .unwrap(),
        kept_mtime,
        "restore must preserve mtimes"
    );
    assert_eq!(
        fs::metadata(root.join("sub/artifact.bin"))
            .unwrap()
            .modified()
            .unwrap(),
        artifact_mtime,
        "restore must preserve mtimes"
    );
}

#[test]
fn restore_is_repeatable() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("file.txt"), "original").unwrap();

    let snapshot = Snapshot::capture(root).unwrap();

    for _ in 0..2 {
        fs::write(root.join("file.txt"), "scribbled").unwrap();
        snapshot.restore_to(root).unwrap();
        assert_eq!(
            fs::read_to_string(root.join("file.txt")).unwrap(),
            "original"
        );
    }
}
