use std::fs;
use std::path::{Path, PathBuf};

use figment::Figment;
use figment::providers::{Format, Serialized, Toml};
use serde::{Deserialize, Serialize};

/// Fields valid in `~/.wezel/config.toml` (global scope).
/// `server_url` is intentionally absent — it must be set per-project.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    pub username: Option<String>,
}

/// Fields valid in `.wezel/config.toml` (project scope).
#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    /// Stable project identity (generated once by `wezel setup`).
    pub project_id: uuid::Uuid,
    /// Human-readable project name.
    pub name: String,
    pub server_url: Option<String>,
    pub username: Option<String>,
    /// Override where pheromone binaries are stored (default: `{exe_dir}/pheromones/`).
    pub pheromone_dir: Option<String>,
    /// Override where the event queue is stored (default: `~/.wezel/queue/`).
    pub queue_dir: Option<String>,
    /// List of registry URIs for experiment adapters.
    /// Each entry can be any valid URI (https://, file://, etc.).
    pub registries: Option<Vec<String>>,
}

/// Fully resolved configuration after merging all layers.
#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub project_id: uuid::Uuid,
    pub name: String,
    pub server_url: String,
    pub username: String,
    /// Where pheromone binaries live.
    pub pheromone_dir: Option<String>,
    /// Where queued events live.
    pub queue_dir: Option<String>,
    /// Configured registry URIs.
    pub registries: Vec<String>,
}

/// Walk up from `start` looking for a `.wezel/config.toml`.
/// Returns `(project_wezel_dir, merged Config)` if found **and** `server_url` is set.
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
                "loaded config: server_url={}, username={}",
                config.server_url,
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
///   3. project `.wezel/config.toml` (server_url + username)
///
/// `project_id` is read directly from the project config — it is never
/// inherited from the global config or defaults.
///
/// Returns `None` if `project_id` or `server_url` is missing.
fn load(project_config_path: &Path) -> Option<Config> {
    let project_raw = fs::read_to_string(project_config_path).ok()?;
    // project_id is not mergeable — read it straight from the project file.
    let project_toml: ProjectConfig = toml::from_str(&project_raw).ok()?;
    let project_id = project_toml.project_id;
    let name = project_toml.name.clone();

    // Merge the remaining (mergeable) fields via figment.
    let mut figment = Figment::new()
        .merge(Serialized::default("username", whoami::username()));

    let global_path = global_config_path();
    if global_path.is_file() {
        log::debug!("merging global config from {}", global_path.display());
        if let Ok(contents) = fs::read_to_string(&global_path)
            && let Ok(global) = toml::from_str::<GlobalConfig>(&contents)
        {
            if let Some(ref u) = global.username {
                figment = figment.merge(Serialized::default("username", u));
            }
        }
    }

    figment = figment.merge(Toml::string(&project_raw));

    let resolved: ProjectConfig = figment.extract().ok()?;

    let server_url = resolved.server_url?;
    if server_url.is_empty() {
        return None;
    }

    Some(Config {
        project_id,
        name,
        server_url,
        username: resolved
            .username
            .filter(|s| !s.is_empty())
            .unwrap_or_else(whoami::username),
        pheromone_dir: resolved.pheromone_dir,
        queue_dir: resolved.queue_dir,
        registries: resolved.registries.unwrap_or_default(),
    })
}

pub fn global_config_path() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".wezel")
        .join("config.toml")
}
