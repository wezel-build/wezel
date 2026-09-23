use std::fs;
use std::path::Path;

use wezel_bench::{Workspace, fetch};

fn symlink_file(target: &Path, link: &Path) {
    #[cfg(unix)]
    std::os::unix::fs::symlink(target, link).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_file(target, link).unwrap();
}

#[test]
fn resolves_project_alias_only_when_it_targets_the_locked_install() {
    let project = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    fs::create_dir_all(project.path().join(".wezel/tools")).unwrap();
    fs::write(
        project.path().join(".wezel/config.toml"),
        format!(
            r#"
project_id = "{}"
name = "executor-test"
[tools.foragers.ir]
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
[tools.foragers.ir]
github = "example/measurements"
tag = "v1"
[tools.foragers.ir.assets]
"{target}" = "sha256:{sha}"
"#
        ),
    )
    .unwrap();
    let ws = Workspace::discover(project.path().into(), store.path().into()).unwrap();
    assert!(ws.resolve_plugin("ir").is_none());

    let unrelated_dir = store.path().join("unlocked-version");
    fs::create_dir(&unrelated_dir).unwrap();
    let unrelated = unrelated_dir.join("publisher-binary");
    fs::write(&unrelated, "unlocked").unwrap();
    let link = ws.executor_path("ir").unwrap();
    symlink_file(&unrelated, &link);
    assert!(ws.resolve_plugin("ir").is_none());

    fs::remove_file(&link).unwrap();
    let install_dir = store.path().join(&sha);
    fs::create_dir(&install_dir).unwrap();
    let published = install_dir.join("measure-ir");
    fs::write(&published, "locked").unwrap();
    symlink_file(&published, &link);

    assert_eq!(ws.resolve_plugin("ir"), Some(link));
}

#[test]
fn workspace_discovery_makes_relative_project_dir_absolute() {
    let current_dir = std::env::current_dir().unwrap();
    let project = tempfile::Builder::new()
        .prefix("wezel-relative-workspace-")
        .tempdir_in(&current_dir)
        .unwrap();
    let store = tempfile::tempdir().unwrap();
    fs::create_dir(project.path().join(".wezel")).unwrap();
    fs::write(
        project.path().join(".wezel/config.toml"),
        format!(
            "project_id = \"{}\"\nname = \"relative-project\"\n",
            uuid::Uuid::new_v4()
        ),
    )
    .unwrap();

    let relative_project = project.path().strip_prefix(&current_dir).unwrap();
    let workspace = Workspace::discover(relative_project.into(), store.path().into()).unwrap();

    assert!(workspace.project_dir.is_absolute());
    assert_eq!(
        workspace.project_dir,
        project.path().canonicalize().unwrap()
    );
}

#[test]
fn rejects_tool_names_that_can_escape_the_project_tools_directory() {
    assert!(wezel_bench::workspace::is_valid_tool_name("filesize.prod"));
    for invalid in ["", ".", "..", ".hidden", "../filesize", "tools/filesize"] {
        assert!(!wezel_bench::workspace::is_valid_tool_name(invalid));
    }
}
