//! `Workspace` — explicit per-invocation state.
//!
//! Bundles the project directory, the local plugin store, and the loaded
//! project config. Wezel is moot without a config, so `Workspace::discover`
//! fails when one isn't found.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::ProjectConfig;

#[derive(Debug)]
pub struct Workspace {
    pub project_dir: PathBuf,
    /// Where forager binaries live. Tests pass a tempdir; the CLI passes
    /// the dir of the running wezel binary.
    pub plugin_dir: PathBuf,
    pub config: ProjectConfig,
}

impl Workspace {
    /// Load `.wezel/config.toml` from `project_dir` and pair it with the
    /// caller-chosen plugin store directory.
    pub fn discover(project_dir: PathBuf, plugin_dir: PathBuf) -> Result<Self> {
        let canonical_project_dir = std::fs::canonicalize(&project_dir)?;
        let config = ProjectConfig::load(&canonical_project_dir)?;
        Ok(Self {
            project_dir,
            plugin_dir,
            config,
        })
    }

    /// Resolve the absolute path of a forager binary in the local store, or
    /// `None` if it isn't installed.
    pub fn resolve_plugin(&self, forager: &str) -> Option<PathBuf> {
        let candidate = self.plugin_dir.join(format!("forager-{forager}"));
        candidate.is_file().then_some(candidate)
    }

    /// Path to the cached `--schema` JSON sidecar for a forager. Written by
    /// the installer; read by lint so we don't shell out per-invocation.
    pub fn schema_path(&self, forager: &str) -> PathBuf {
        self.plugin_dir
            .join(format!("forager-{forager}.schema.json"))
    }

    /// Default plugin store: the directory containing the running wezel
    /// binary. Used by the CLI; tests should pass a tempdir to `discover`
    /// directly.
    pub fn default_plugin_dir() -> Result<PathBuf> {
        std::env::current_exe()
            .context("locating current exe")?
            .parent()
            .map(|p| p.to_path_buf())
            .context("current exe has no parent directory")
    }
}

/// Per-run isolated checkout. Foragers run inside this directory so a build's
/// `target/` never leaks into the user's working tree and step patches don't
/// touch tracked files in-place.
pub struct Scratch {
    dir: tempfile::TempDir,
}

impl Scratch {
    /// Clone `source` into a fresh tempdir and check out `commit_sha`
    /// detached. Local clones use git's default hardlinking when possible, so
    /// this is much cheaper than a network clone.
    pub fn create(source: &Path, commit_sha: &str) -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("wezel-scratch-")
            .tempdir()
            .context("creating scratch tempdir")?;

        let status = Command::new("git")
            .arg("clone")
            .arg("--local")
            .arg("--no-checkout")
            .arg(source)
            .arg(dir.path())
            .status()
            .context("spawning git clone")?;
        if !status.success() {
            bail!("git clone into {} failed", dir.path().display());
        }

        let status = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .arg("checkout")
            .arg("--detach")
            .arg(commit_sha)
            .status()
            .context("spawning git checkout")?;
        if !status.success() {
            bail!("git checkout {commit_sha} in scratch failed");
        }

        Ok(Self { dir })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }
}

/// Side-stash of a project directory's contents, used to make sampled steps
/// i.i.d. by restoring before each iteration.
pub struct Snapshot {
    holder: tempfile::TempDir,
}

impl Snapshot {
    /// `cp -a source <holder>/snap`. Captures the full tree (including
    /// `target/`, `.git/`, etc.) so a restore returns to byte-identical state.
    pub fn capture(source: &Path) -> Result<Self> {
        let holder = tempfile::Builder::new()
            .prefix("wezel-snapshot-")
            .tempdir()
            .context("creating snapshot tempdir")?;
        let dest = holder.path().join("snap");
        let status = Command::new("cp")
            .arg("-a")
            .arg(source)
            .arg(&dest)
            .status()
            .context("spawning cp -a for snapshot")?;
        if !status.success() {
            bail!("cp -a {} {} failed", source.display(), dest.display());
        }
        Ok(Self { holder })
    }

    /// Wipe `target` and copy the snapshot's contents back in. `target` itself
    /// is preserved (so external owners — like a `Scratch`'s `TempDir` — keep
    /// their invariant).
    pub fn restore_to(&self, target: &Path) -> Result<()> {
        for entry in std::fs::read_dir(target)
            .with_context(|| format!("reading {} for restore", target.display()))?
        {
            let entry = entry?;
            let p = entry.path();
            let ft = entry.file_type()?;
            if ft.is_dir() && !ft.is_symlink() {
                std::fs::remove_dir_all(&p).with_context(|| format!("removing {}", p.display()))?;
            } else {
                std::fs::remove_file(&p).with_context(|| format!("removing {}", p.display()))?;
            }
        }
        let mut snap_contents = self.holder.path().join("snap").into_os_string();
        snap_contents.push("/.");
        let status = Command::new("cp")
            .arg("-a")
            .arg(&snap_contents)
            .arg(target)
            .status()
            .context("spawning cp -a for restore")?;
        if !status.success() {
            bail!("cp -a {:?} {} failed", snap_contents, target.display());
        }
        Ok(())
    }
}
