use std::fs;
use std::path::{Path, PathBuf};

use figment::Figment;
use figment::providers::{Format, Serialized, Toml};
use serde::{Deserialize, Serialize};

/// Fields valid in `~/.wezel/config.toml` (global scope).
/// `burrow_url` is intentionally absent — it must be set per-project.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub username: Option<String>,
}

/// Fields valid in `.wezel/config.toml` (project scope).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub burrow_url: Option<String>,
    pub username: Option<String>,
}

/// Fully resolved configuration after merging all layers.
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub burrow_url: String,
    pub username: String,
}

/// Walk up from `start` looking for a `.wezel/config.toml`.
/// Returns `(project_wezel_dir, merged Config)` if found **and** `burrow_url` is set.
pub fn discover(start: &Path) -> Option<(PathBuf, Config)> {
    log::debug!("discovering config from {}", start.display());

    let mut dir = start.to_path_buf();
    loop {
        let candidate = dir.join(".wezel").join("config.toml");
        if candidate.is_file() {
            log::debug!("found project config at {}", candidate.display());
            let wezel_dir = dir.join(".wezel");
            let config = load(&candidate)?;
            log::debug!(
                "loaded config: burrow_url={}, username={}",
                config.burrow_url,
                config.username,
            );
            return Some((wezel_dir, config));
        }
        if !dir.pop() {
            log::debug!("no .wezel/config.toml found in any ancestor");
            return None;
        }
    }
}

/// Build a `Config` by layering:
///   1. defaults (username = whoami)
///   2. `~/.wezel/config.toml`  (global — username only)
///   3. project `.wezel/config.toml` (burrow_url + username)
///
/// Returns `None` if `burrow_url` ends up missing.
fn load(project_config_path: &Path) -> Option<Config> {
    let defaults = ProjectConfig {
        burrow_url: None,
        username: Some(whoami::username()),
    };

    let mut figment = Figment::new().merge(Serialized::defaults(defaults));

    // Layer the global config if it exists.
    let global_path = global_config_path();
    if global_path.is_file() {
        log::debug!("merging global config from {}", global_path.display());
        // Read as GlobalConfig first so we only pick up valid global keys.
        if let Ok(contents) = fs::read_to_string(&global_path) {
            if let Ok(global) = toml::from_str::<GlobalConfig>(&contents) {
                // Only merge username (the only global-scoped key).
                if let Some(ref u) = global.username {
                    figment = figment.merge(Serialized::default("username", u));
                }
            }
        }
    }

    // Layer the project config on top.
    figment = figment.merge(Toml::file(project_config_path));

    let resolved: ProjectConfig = figment.extract().ok()?;

    let burrow_url = resolved.burrow_url?;
    if burrow_url.is_empty() {
        return None;
    }

    Some(Config {
        burrow_url,
        username: resolved
            .username
            .filter(|s| !s.is_empty())
            .unwrap_or_else(whoami::username),
    })
}

pub fn global_config_path() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".wezel")
        .join("config.toml")
}
