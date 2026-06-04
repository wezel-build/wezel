//! `Scratch::create_with_worktree` must carry the source repo's uncommitted
//! working-tree state into the clone so `wezel experiment run` measures the
//! current checkout — not just committed HEAD.

use std::fs;
use std::path::Path;
use std::process::Command;

use wezel_bench::workspace::Scratch;

fn git(repo: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(repo)
        .args(args)
        .status()
        .expect("spawning git");
    assert!(status.success(), "git {args:?} failed");
}

/// A repo with one committed file and a deterministic identity/HEAD.
fn init_repo(dir: &Path) -> String {
    git(dir, &["init", "--quiet"]);
    git(dir, &["config", "user.email", "t@example.com"]);
    git(dir, &["config", "user.name", "test"]);
    fs::write(dir.join("tracked.txt"), "v1\n").unwrap();
    fs::write(dir.join(".gitignore"), "ignored.txt\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "--quiet", "-m", "init"]);
    let out = Command::new("git")
        .current_dir(dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn overlay_carries_modified_untracked_and_deletions() {
    let src = tempfile::tempdir().unwrap();
    let head = init_repo(src.path());

    // Dirty the working tree in four ways.
    fs::write(src.path().join("tracked.txt"), "v2-uncommitted\n").unwrap(); // modified
    fs::write(src.path().join("untracked.txt"), "new\n").unwrap(); // untracked
    fs::write(src.path().join("ignored.txt"), "junk\n").unwrap(); // gitignored
    fs::create_dir(src.path().join("nested")).unwrap();
    fs::write(src.path().join("nested/deep.txt"), "deep\n").unwrap(); // untracked, nested

    let scratch = Scratch::create_with_worktree(src.path(), &head).unwrap();
    let p = scratch.path();

    // Modified tracked file reflects the worktree, not HEAD.
    assert_eq!(
        fs::read_to_string(p.join("tracked.txt")).unwrap(),
        "v2-uncommitted\n"
    );
    // Untracked files are carried over, including nested ones.
    assert_eq!(
        fs::read_to_string(p.join("untracked.txt")).unwrap(),
        "new\n"
    );
    assert_eq!(
        fs::read_to_string(p.join("nested/deep.txt")).unwrap(),
        "deep\n"
    );
    // Gitignored files are excluded.
    assert!(!p.join("ignored.txt").exists());
}

#[test]
fn overlay_drops_worktree_deletions() {
    let src = tempfile::tempdir().unwrap();
    let head = init_repo(src.path());

    fs::remove_file(src.path().join("tracked.txt")).unwrap(); // deleted in worktree

    let scratch = Scratch::create_with_worktree(src.path(), &head).unwrap();
    assert!(!scratch.path().join("tracked.txt").exists());
}

#[test]
fn plain_create_ignores_worktree() {
    let src = tempfile::tempdir().unwrap();
    let head = init_repo(src.path());

    fs::write(src.path().join("tracked.txt"), "v2-uncommitted\n").unwrap();
    fs::write(src.path().join("untracked.txt"), "new\n").unwrap();

    let scratch = Scratch::create(src.path(), &head).unwrap();
    let p = scratch.path();
    // Clean checkout: committed content only.
    assert_eq!(fs::read_to_string(p.join("tracked.txt")).unwrap(), "v1\n");
    assert!(!p.join("untracked.txt").exists());
}
