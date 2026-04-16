use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Create a temporary directory with a unique name under the system temp dir.
fn make_tempdir(prefix: &str) -> PathBuf {
    let id = uuid::Uuid::new_v4();
    let dir = std::env::temp_dir().join(format!("{prefix}-{id}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Run a git command in `dir`, panicking on failure.
fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .output()
        .unwrap_or_else(|e| panic!("git {} failed to spawn: {e}", args.join(" ")));
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        panic!("git {} failed: {stderr}", args.join(" "));
    }
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Create a fake forager plugin script that writes a configurable metric value.
/// The value is read from a file at `<repo>/.test-metric-value`.
fn create_fake_plugin(dir: &Path) -> PathBuf {
    let script_path = dir.join("forager-test-metric");
    let script = r#"#!/bin/sh
# Fake forager plugin for testing.
# Reads the metric value from .test-metric-value in cwd (defaults to 100).
VALUE=$(cat .test-metric-value 2>/dev/null || echo 100)
cat > "$FORAGER_OUT" <<ENDJSON
{
  "measurements": [
    {
      "name": "test-metric",
      "value": $VALUE,
      "tags": {}
    }
  ]
}
ENDJSON
"#;
    fs::write(&script_path, script).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    script_path
}

struct TestFixture {
    bare_dir: PathBuf,
    work_dir: PathBuf,
    plugin_dir: PathBuf,
}

impl TestFixture {
    fn new() -> Self {
        let bare_dir = make_tempdir("wezel-test-bare");
        let work_dir = make_tempdir("wezel-test-work");
        let plugin_dir = make_tempdir("wezel-test-plugins");

        // Create bare remote with explicit main branch.
        git(&bare_dir, &["init", "--bare", "--initial-branch=main"]);

        // Clone it.
        git(
            work_dir.parent().unwrap(),
            &["clone", bare_dir.to_str().unwrap(), work_dir.to_str().unwrap()],
        );

        // Configure the clone.
        git(&work_dir, &["config", "user.name", "test"]);
        git(&work_dir, &["config", "user.email", "test@test.com"]);
        // Ensure the local branch is called "main".
        git(&work_dir, &["checkout", "-b", "main"]);

        // Create the fake forager plugin.
        create_fake_plugin(&plugin_dir);

        // Create .wezel/config.toml (no server_url — standalone mode).
        let wezel_dir = work_dir.join(".wezel");
        fs::create_dir_all(&wezel_dir).unwrap();
        fs::write(
            wezel_dir.join("config.toml"),
            format!(
                "project_id = \"{}\"\nname = \"test-project\"\n",
                uuid::Uuid::new_v4()
            ),
        )
        .unwrap();

        // Create an experiment that uses our fake plugin.
        let exp_dir = wezel_dir.join("experiments").join("basic");
        fs::create_dir_all(&exp_dir).unwrap();
        fs::write(
            exp_dir.join("experiment.toml"),
            r#"
name = "basic"
description = "Test experiment"

[[steps]]
name = "measure"
tool = "test-metric"

[[summaries]]
name = "total"
measurement = "test-metric"
aggregation = "sum"
bisect = true
"#,
        )
        .unwrap();

        // Set the initial metric value.
        fs::write(work_dir.join(".test-metric-value"), "100").unwrap();

        // Commit everything.
        git(&work_dir, &["add", "-A"]);
        git(&work_dir, &["commit", "-m", "initial"]);
        git(&work_dir, &["push", "origin", "main"]);

        Self {
            bare_dir,
            work_dir,
            plugin_dir,
        }
    }

    /// Add the fake plugin to PATH for the duration of a closure.
    fn with_plugin_on_path<F, R>(&self, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let original_path = std::env::var("PATH").unwrap_or_default();
        let new_path = format!("{}:{original_path}", self.plugin_dir.display());
        // SAFETY: These tests run with --test-threads=1, so no concurrent
        // env mutation.
        unsafe { std::env::set_var("PATH", &new_path) };
        let result = f();
        unsafe { std::env::set_var("PATH", &original_path) };
        result
    }

    /// Make a commit with a new metric value to simulate a regression.
    /// Ensures we're on main, commits, and pushes.
    fn commit_with_metric(&self, value: u64, message: &str) {
        // standalone may have left us on a detached HEAD — get back to main.
        let _ = Command::new("git")
            .args(["checkout", "main"])
            .current_dir(&self.work_dir)
            .output();
        // Pull any changes from remote (standalone may have pushed data branch).
        let _ = Command::new("git")
            .args(["pull", "--ff-only", "origin", "main"])
            .current_dir(&self.work_dir)
            .output();
        fs::write(self.work_dir.join(".test-metric-value"), value.to_string()).unwrap();
        git(&self.work_dir, &["add", ".test-metric-value"]);
        git(&self.work_dir, &["commit", "-m", message]);
        git(&self.work_dir, &["push", "origin", "main"]);
    }
}

impl Drop for TestFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.bare_dir);
        let _ = fs::remove_dir_all(&self.work_dir);
        let _ = fs::remove_dir_all(&self.plugin_dir);
    }
}

#[test]
fn standalone_creates_baseline_on_first_run() {
    let fixture = TestFixture::new();

    let report = fixture.with_plugin_on_path(|| {
        wezel_bench::standalone::run_standalone(
            &fixture.work_dir,
            "wezel/data",
            "main",
            10.0,
            None,
        )
        .unwrap()
    });

    assert_eq!(report.results.len(), 1);
    let result = &report.results[0];
    assert_eq!(result.experiment, "basic");
    // Serialize to check the action string representation.
    let json = serde_json::to_value(result).unwrap();
    assert_eq!(json["action"], "baseline_created");

    // Verify the data branch was created with a baseline file.
    let baseline_raw = git(
        &fixture.work_dir,
        &["show", "origin/wezel/data:baselines/basic.json"],
    );
    let baseline: serde_json::Value = serde_json::from_str(&baseline_raw).unwrap();
    assert_eq!(baseline["summaries"]["total"], 100.0);
}

#[test]
fn standalone_updates_baseline_when_no_regression() {
    let fixture = TestFixture::new();

    // First run: create baseline.
    fixture.with_plugin_on_path(|| {
        wezel_bench::standalone::run_standalone(
            &fixture.work_dir,
            "wezel/data",
            "main",
            10.0,
            None,
        )
        .unwrap()
    });

    // Add a commit with a small change (within threshold).
    fixture.commit_with_metric(105, "small change");

    // Second run: should update baseline.
    let report = fixture.with_plugin_on_path(|| {
        wezel_bench::standalone::run_standalone(
            &fixture.work_dir,
            "wezel/data",
            "main",
            10.0,
            None,
        )
        .unwrap()
    });

    assert_eq!(report.results.len(), 1);
    let json = serde_json::to_value(&report.results[0]).unwrap();
    assert_eq!(json["action"], "baseline_updated");

    // Verify the baseline was updated.
    git(&fixture.work_dir, &["fetch", "origin"]);
    let baseline_raw = git(
        &fixture.work_dir,
        &["show", "origin/wezel/data:baselines/basic.json"],
    );
    let baseline: serde_json::Value = serde_json::from_str(&baseline_raw).unwrap();
    assert_eq!(baseline["summaries"]["total"], serde_json::json!(105.0));
}

#[test]
fn standalone_detects_regression_and_bisects() {
    let fixture = TestFixture::new();

    // First run: create baseline at 100.
    fixture.with_plugin_on_path(|| {
        wezel_bench::standalone::run_standalone(
            &fixture.work_dir,
            "wezel/data",
            "main",
            10.0,
            None,
        )
        .unwrap()
    });

    // Add commits: innocent (small changes), then REGRESSOR.
    fixture.commit_with_metric(101, "innocent-1");
    fixture.commit_with_metric(102, "innocent-2");
    fixture.commit_with_metric(200, "regressor");

    // Second run: should detect regression.
    let report = fixture.with_plugin_on_path(|| {
        wezel_bench::standalone::run_standalone(
            &fixture.work_dir,
            "wezel/data",
            "main",
            10.0,
            None,
        )
        .unwrap()
    });

    assert_eq!(report.results.len(), 1);
    let json = serde_json::to_value(&report.results[0]).unwrap();
    assert_eq!(json["action"], "regression_detected");
    assert!(json["details"]["regression_pct"].as_f64().unwrap() > 10.0);

    // Verify bisection state was written.
    let bisect_raw = git(
        &fixture.work_dir,
        &["show", "origin/wezel/data:bisection/active/basic.json"],
    );
    let bisect: serde_json::Value = serde_json::from_str(&bisect_raw).unwrap();
    assert_eq!(bisect["experiment"], "basic");
    assert_eq!(bisect["summary_name"], "total");

    // Run bisection steps until culprit is found.
    let mut found_culprit = false;
    for _ in 0..10 {
        let report = fixture.with_plugin_on_path(|| {
            wezel_bench::standalone::run_standalone(
                &fixture.work_dir,
                "wezel/data",
                "main",
                10.0,
                None,
            )
            .unwrap()
        });

        let json = serde_json::to_value(&report.results[0]).unwrap();
        let action = json["action"].as_str().unwrap();

        if action == "culprit_found" {
            found_culprit = true;
            assert_eq!(
                json["details"]["culprit_message"].as_str().unwrap(),
                "regressor"
            );
            break;
        }

        assert_eq!(action, "bisect_step");
    }

    assert!(found_culprit, "bisection did not converge");
}
