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

    /// Install a fake forager binary into the plugin store. The fake handles
    /// `--schema` so lint's schema check succeeds.
    fn install_fake_forager(&self, name: &str) {
        let path = self.plugin_dir.join(format!("forager-{name}"));
        let script = r#"#!/bin/sh
if [ "$1" = "--schema" ]; then
  printf '{"name":"%s","output":{}}\n' "${0##*/forager-}"
  exit 0
fi
exit 0
"#;
        fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    fn workspace(&self) -> Workspace {
        Workspace::discover(self.project_dir.clone(), self.plugin_dir.clone())
            .expect("workspace discovery")
    }

    fn run_lint(&self) -> anyhow::Result<()> {
        let ws = self.workspace();
        wezel_bench::lint::run_lint(&ws, None)
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
fn lint_fails_when_forager_not_declared_in_config() {
    let fx = LintFixture::new(""); // no [tools] section at all
    fx.add_experiment("e1", &experiment_with_step("exec", "cmd = \"true\""));
    let err = fx.run_lint().unwrap_err().to_string();
    assert!(err.contains("error"), "expected lint to fail, got: {err}");
}

#[test]
fn lint_fails_even_when_binary_present_but_config_missing() {
    // The bug we hit: a stale binary in the store made lint pass despite the
    // missing config declaration. Make sure that no longer happens.
    let fx = LintFixture::new(""); // no [tools] section
    fx.install_fake_forager("exec");
    fx.add_experiment("e1", &experiment_with_step("exec", "cmd = \"true\""));
    assert!(
        fx.run_lint().is_err(),
        "lint should fail even when the binary is present, since config is missing"
    );
}

#[test]
fn lint_fails_when_patch_file_missing() {
    let fx = LintFixture::new("[tools.foragers.exec]\ngithub = \"acme/forager_exec\"\n");
    fx.install_fake_forager("exec");
    fx.add_experiment(
        "e1",
        &experiment_with_step("exec", "cmd = \"true\"\napply-diff = true"),
    );
    // No step1.patch file written.
    assert!(fx.run_lint().is_err());
}

#[test]
fn lint_passes_when_declared_and_installed() {
    let fx = LintFixture::new("[tools.foragers.exec]\ngithub = \"acme/forager_exec\"\n");
    fx.install_fake_forager("exec");
    fx.add_experiment("e1", &experiment_with_step("exec", "cmd = \"true\""));
    fx.run_lint()
        .expect("lint should pass on a clean experiment");
}
