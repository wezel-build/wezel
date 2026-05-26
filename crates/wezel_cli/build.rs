use std::process::Command;

fn main() {
    // CI pre-bakes the SHA (e.g. nightly.yml rewrites Cargo.toml before
    // build, which would otherwise be flagged dirty by `git status`). Honour
    // the override so release tarballs aren't mislabelled.
    let sha = std::env::var("WEZEL_BUILD_SHA")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(git_sha)
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=WEZEL_BUILD_SHA={sha}");

    // Rebuild when HEAD moves or any branch ref changes.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs");
    println!("cargo:rerun-if-env-changed=WEZEL_BUILD_SHA");
}

fn git_sha() -> Option<String> {
    let out = Command::new("git")
        .args(["rev-parse", "--short=7", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let sha = String::from_utf8(out.stdout).ok()?.trim().to_string();
    if sha.is_empty() {
        return None;
    }

    let dirty = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .is_some_and(|o| o.status.success() && !o.stdout.is_empty());

    Some(if dirty { format!("{sha}-dirty") } else { sha })
}
