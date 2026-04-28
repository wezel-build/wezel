//! Wezel project lockfile (`.wezel/wezel.lock`).
//!
//! Pins resolved versions and content hashes for anything declared in
//! `.wezel/config.toml` that needs reproducible re-installs across machines —
//! today, foragers (under `[tools.foragers]`); tomorrow, pheromones, explainer
//! modules, or other declared dependencies.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const FILE_NAME: &str = "wezel.lock";
pub const CURRENT_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct WezelLock {
    /// Schema version for forward-compat. Always `1` today.
    pub version: u32,
    /// Locked tool entries, mirroring `[tools]` in `config.toml`.
    #[serde(default)]
    pub tools: LockedTools,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize)]
pub struct LockedTools {
    #[serde(default)]
    pub foragers: BTreeMap<String, LockedTool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LockedTool {
    /// `owner/repo` mirrored from config for a self-contained lockfile.
    pub github: String,
    /// Pinned release tag.
    pub tag: String,
    /// Map of `target-triple` → archive `sha256:<hex>`. Populated lazily on
    /// first install on a given target.
    #[serde(default)]
    pub assets: BTreeMap<String, String>,
}

pub fn path(project_dir: &Path) -> PathBuf {
    project_dir.join(".wezel").join(FILE_NAME)
}

pub fn load(project_dir: &Path) -> Result<WezelLock> {
    let p = path(project_dir);
    if !p.is_file() {
        return Ok(WezelLock {
            version: CURRENT_VERSION,
            tools: LockedTools::default(),
        });
    }
    let raw = std::fs::read_to_string(&p).with_context(|| format!("reading {}", p.display()))?;
    let lock: WezelLock =
        toml::from_str(&raw).with_context(|| format!("parsing {}", p.display()))?;
    Ok(lock)
}

pub fn save(project_dir: &Path, lock: &WezelLock) -> Result<()> {
    let p = path(project_dir);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let raw = toml::to_string_pretty(lock).context("serialising wezel.lock")?;
    std::fs::write(&p, raw).with_context(|| format!("writing {}", p.display()))?;
    Ok(())
}
