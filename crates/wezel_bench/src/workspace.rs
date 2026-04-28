//! `Workspace` — explicit per-invocation state.
//!
//! Bundles the project directory, the local plugin store, and the loaded
//! project config. Wezel is moot without a config, so `Workspace::discover`
//! fails when one isn't found.

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::Config;

#[derive(Debug)]
pub struct Workspace {
    pub project_dir: PathBuf,
    /// Where forager binaries live. Tests pass a tempdir; the CLI passes
    /// the dir of the running wezel binary.
    pub plugin_dir: PathBuf,
    pub config: Config,
}

impl Workspace {
    /// Load `.wezel/config.toml` from `project_dir` and pair it with the
    /// caller-chosen plugin store directory.
    pub fn discover(project_dir: PathBuf, plugin_dir: PathBuf) -> Result<Self> {
        let config = Config::load(&project_dir)?;
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
