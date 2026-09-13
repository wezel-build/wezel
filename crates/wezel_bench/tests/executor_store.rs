use std::fs;

use wezel_bench::{Workspace, fetch};

#[test]
fn locked_plugins_keep_legacy_installations_and_prefer_renamed_binaries() {
    let project = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    fs::create_dir(project.path().join(".wezel")).unwrap();
    fs::write(
        project.path().join(".wezel/config.toml"),
        format!(
            r#"
project_id = "{}"
name = "executor-test"
[tools.foragers.llvm-lines]
github = "example/measurements"
"#,
            uuid::Uuid::new_v4()
        ),
    )
    .unwrap();
    let target = fetch::current_target().unwrap();
    let sha = "a".repeat(64);
    fs::write(
        project.path().join(".wezel/wezel.lock"),
        format!(
            r#"
version = 1
[tools.foragers.llvm-lines]
github = "example/measurements"
tag = "v1"
[tools.foragers.llvm-lines.assets]
"{target}" = "sha256:{sha}"
"#
        ),
    )
    .unwrap();
    let ws = Workspace::discover(project.path().into(), store.path().into()).unwrap();
    assert!(ws.resolve_plugin("llvm-lines").is_none());

    // An unrelated cached version must never satisfy the pinned hash.
    fs::create_dir(store.path().join("unlocked-version")).unwrap();
    fs::write(
        store.path().join("unlocked-version/wezel_llvm_lines"),
        "unlocked",
    )
    .unwrap();
    assert!(ws.resolve_plugin("llvm-lines").is_none());

    let dir = store.path().join(&sha);
    fs::create_dir(&dir).unwrap();
    let legacy = dir.join("forager-llvm-lines");
    fs::write(&legacy, "legacy").unwrap();
    fs::write(Workspace::schema_sidecar_path(&legacy), "legacy schema").unwrap();
    let resolved = ws.resolve_plugin("llvm-lines").unwrap();
    assert_eq!(resolved, legacy);
    assert_eq!(
        fs::read_to_string(Workspace::schema_sidecar_path(&resolved)).unwrap(),
        "legacy schema"
    );

    let renamed = dir.join("wezel_llvm_lines");
    assert_eq!(ws.plugin_path("llvm-lines", &sha), renamed);
    fs::write(&renamed, "renamed").unwrap();
    fs::write(Workspace::schema_sidecar_path(&renamed), "new schema").unwrap();
    let resolved = ws.resolve_plugin("llvm-lines").unwrap();
    assert_eq!(resolved, renamed);
    assert_eq!(
        fs::read_to_string(Workspace::schema_sidecar_path(&resolved)).unwrap(),
        "new schema"
    );
}
