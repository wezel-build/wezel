//! Tests for `wezel experiment lint`.

use std::fs;
use std::path::PathBuf;

use wezel_bench::Workspace;

fn make_tempdir(prefix: &str) -> PathBuf {
    let id = uuid::Uuid::new_v4();
    let dir = std::env::temp_dir().join(format!("{prefix}-{id}"));
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct LintFixture {
    project_dir: PathBuf,
    plugin_dir: PathBuf,
}

impl LintFixture {
    fn new(config_toml: &str) -> Self {
        let project_dir = make_tempdir("wezel-test-lint");
        let plugin_dir = make_tempdir("wezel-test-lint-plugins");
        let wezel_dir = project_dir.join(".wezel");
        fs::create_dir_all(wezel_dir.join("experiments")).unwrap();
        fs::write(
            wezel_dir.join("config.toml"),
            format!(
                "project_id = \"{}\"\nname = \"test\"\n{config_toml}",
                uuid::Uuid::new_v4()
            ),
        )
        .unwrap();
        Self {
            project_dir,
            plugin_dir,
        }
    }

    fn add_experiment(&self, name: &str, toml: &str) -> PathBuf {
        let dir = self.project_dir.join(".wezel/experiments").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("experiment.toml"), toml).unwrap();
        dir
    }

    /// Install a fake forager binary into the plugin store, plus the schema
    /// sidecar that the real installer would have written.
    fn install_fake_forager(&self, name: &str) {
        let path = self.plugin_dir.join(format!("forager-{name}"));
        fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let schema_path = self.plugin_dir.join(format!("forager-{name}.schema.json"));
        fs::write(
            &schema_path,
            format!(r#"{{"name":"{name}","output":{{}}}}"#),
        )
        .unwrap();
    }

    /// Write a minimal lockfile entry for `name` so lint's hard-fail-on-
    /// missing-lockfile gate is satisfied.
    fn lock_forager(&self, name: &str) {
        let lock_path = self.project_dir.join(".wezel/wezel.lock");
        let body = format!(
            r#"version = 1

[tools.foragers.{name}]
github = "acme/forager_{name}"
tag = "v0.0.0"
"#,
            name = name,
        );
        fs::write(&lock_path, body).unwrap();
    }

    fn workspace(&self) -> Workspace {
        Workspace::discover(self.project_dir.clone(), self.plugin_dir.clone())
            .expect("workspace discovery")
    }

    fn run_lint(&self) -> anyhow::Result<()> {
        let ws = self.workspace();
        wezel_bench::lint::run_lint(&ws, None)
    }

    /// Convenience: write a minimal lockfile that satisfies the "no lockfile
    /// = hard fail" gate for tests that don't otherwise care.
    fn write_empty_lockfile(&self) {
        let lock_path = self.project_dir.join(".wezel/wezel.lock");
        fs::write(lock_path, "version = 1\n").unwrap();
    }
}

impl Drop for LintFixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.project_dir);
        let _ = fs::remove_dir_all(&self.plugin_dir);
    }
}

fn experiment_with_step(tool: &str, extra: &str) -> String {
    format!(
        r#"description = "test"
[[steps]]
name = "step1"
tool = "{tool}"
{extra}
"#
    )
}

#[test]
fn lint_fails_when_lockfile_missing() {
    let fx = LintFixture::new("[tools.foragers.exec]\ngithub = \"acme/forager_exec\"\n");
    fx.install_fake_forager("exec");
    fx.add_experiment("e1", &experiment_with_step("exec", "cmd = \"true\""));
    // Note: no lockfile written.
    let err = fx.run_lint().unwrap_err().to_string();
    assert!(
        err.contains("lockfile") || err.contains("wezel.lock"),
        "expected hard-fail mentioning the lockfile, got: {err}"
    );
}

#[test]
fn lint_fails_when_forager_not_declared_in_config() {
    let fx = LintFixture::new(""); // no [tools] section at all
    fx.write_empty_lockfile();
    fx.add_experiment("e1", &experiment_with_step("exec", "cmd = \"true\""));
    let err = fx.run_lint().unwrap_err().to_string();
    assert!(err.contains("error"), "expected lint to fail, got: {err}");
}

#[test]
fn lint_fails_even_when_binary_present_but_config_missing() {
    // The bug we hit: a stale binary in the store made lint pass despite the
    // missing config declaration. Make sure that no longer happens.
    let fx = LintFixture::new(""); // no [tools] section
    fx.write_empty_lockfile();
    fx.install_fake_forager("exec");
    fx.add_experiment("e1", &experiment_with_step("exec", "cmd = \"true\""));
    assert!(
        fx.run_lint().is_err(),
        "lint should fail even when the binary is present, since config is missing"
    );
}

#[test]
fn lint_fails_when_forager_declared_but_not_locked() {
    let fx = LintFixture::new("[tools.foragers.exec]\ngithub = \"acme/forager_exec\"\n");
    fx.write_empty_lockfile(); // lockfile exists but has no foragers
    fx.install_fake_forager("exec");
    fx.add_experiment("e1", &experiment_with_step("exec", "cmd = \"true\""));
    let err = fx.run_lint().unwrap_err().to_string();
    assert!(
        err.contains("error"),
        "expected lint to fail when forager isn't pinned, got: {err}"
    );
}

#[test]
fn lint_fails_when_patch_file_missing() {
    let fx = LintFixture::new("[tools.foragers.exec]\ngithub = \"acme/forager_exec\"\n");
    fx.lock_forager("exec");
    fx.install_fake_forager("exec");
    fx.add_experiment(
        "e1",
        &experiment_with_step("exec", "cmd = \"true\"\napply-diff = true"),
    );
    // No step1.patch file written.
    assert!(fx.run_lint().is_err());
}

#[test]
fn lint_fails_when_schema_sidecar_missing() {
    let fx = LintFixture::new("[tools.foragers.exec]\ngithub = \"acme/forager_exec\"\n");
    fx.lock_forager("exec");
    // Install just the binary, no sidecar.
    let path = fx.plugin_dir.join("forager-exec");
    fs::write(&path, "#!/bin/sh\nexit 0\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    fx.add_experiment("e1", &experiment_with_step("exec", "cmd = \"true\""));
    assert!(
        fx.run_lint().is_err(),
        "lint should fail when the cached schema sidecar is missing"
    );
}

#[test]
fn lint_passes_when_declared_locked_and_installed() {
    let fx = LintFixture::new("[tools.foragers.exec]\ngithub = \"acme/forager_exec\"\n");
    fx.lock_forager("exec");
    fx.install_fake_forager("exec");
    fx.add_experiment("e1", &experiment_with_step("exec", "cmd = \"true\""));
    fx.run_lint()
        .expect("lint should pass on a clean experiment");
}
