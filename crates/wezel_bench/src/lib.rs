pub mod daemon;
pub mod fetch;
pub mod lint;
pub mod new;
pub mod run;
pub mod standalone;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use wezel_types::{Aggregation, ExperimentDef, ForagerPluginEnvelope, StepDef, SummaryDef};

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ProjectConfig {
    pub project_id: uuid::Uuid,
    pub name: String,
    pub server_url: Option<String>,
    pub data_branch: Option<String>,
    #[serde(default)]
    pub plugins: HashMap<String, String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub project_id: uuid::Uuid,
    pub name: String,
    pub server_url: Option<String>,
    /// Branch used for standalone state storage (default: "wezel/data").
    pub data_branch: String,
    /// Plugin sources. Maps plugin name to source URI (e.g. "github:owner/repo").
    pub plugins: HashMap<String, String>,
}

impl Config {
    pub fn load(project_dir: &Path) -> Result<Config> {
        let config_path = project_dir.join(".wezel").join("config.toml");
        if !config_path.is_file() {
            bail!("no .wezel/config.toml found at {}", config_path.display());
        }
        let raw = std::fs::read_to_string(&config_path)
            .with_context(|| format!("reading {}", config_path.display()))?;
        let resolved: ProjectConfig =
            toml::from_str(&raw).with_context(|| format!("parsing {}", config_path.display()))?;
        // server_url: env var takes precedence, then config file.
        let server_url = std::env::var("WEZEL_BURROW_URL")
            .ok()
            .filter(|s| !s.is_empty())
            .or_else(|| resolved.server_url.filter(|s| !s.is_empty()));
        Ok(Config {
            project_id: resolved.project_id,
            name: resolved.name,
            server_url,
            data_branch: resolved
                .data_branch
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "wezel/data".to_string()),
            plugins: resolved.plugins,
        })
    }
}

// ── Experiment TOML parsing ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct ExperimentToml {
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<ExperimentStepToml>,
    #[serde(default)]
    pub summaries: Vec<SummaryToml>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum DiffField {
    Bool(bool),
    Name(String),
}

#[derive(Debug, Deserialize)]
struct ExperimentStepToml {
    name: String,
    tool: Option<String>,
    description: Option<String>,
    #[serde(rename = "apply-diff")]
    apply_diff: Option<DiffField>,
    #[serde(flatten)]
    rest: HashMap<String, toml::Value>,
}

#[derive(Debug, Deserialize)]
struct SummaryToml {
    name: String,
    measurement: String,
    aggregation: Aggregation,
    #[serde(default)]
    filter: HashMap<String, String>,
    #[serde(default = "bool_true")]
    bisect: bool,
}

fn bool_true() -> bool {
    true
}

pub fn parse_experiment(experiment_dir: &Path) -> Result<ExperimentDef> {
    let toml_path = experiment_dir.join("experiment.toml");
    let raw = std::fs::read_to_string(&toml_path)
        .with_context(|| format!("reading {}", toml_path.display()))?;
    let experiment: ExperimentToml =
        toml::from_str(&raw).with_context(|| format!("parsing {}", toml_path.display()))?;

    let mut steps = Vec::with_capacity(experiment.steps.len());
    for raw_step in experiment.steps {
        let forager = match raw_step.tool {
            Some(f) => f,
            None if raw_step.rest.contains_key("cmd") => "exec".to_string(),
            None => bail!("step '{}' has no tool name and no cmd field", raw_step.name),
        };

        let inputs_map: serde_json::Map<String, serde_json::Value> = raw_step
            .rest
            .into_iter()
            .map(|(k, v)| Ok((k, toml_to_json(v)?)))
            .collect::<Result<_>>()?;

        let diff = match raw_step.apply_diff {
            Some(DiffField::Bool(true)) => Some(raw_step.name.clone()),
            Some(DiffField::Bool(false)) | None => None,
            Some(DiffField::Name(s)) => Some(s),
        };

        steps.push(StepDef {
            name: raw_step.name,
            forager,
            description: raw_step.description,
            diff,
            inputs: serde_json::Value::Object(inputs_map),
        });
    }

    let summaries = experiment
        .summaries
        .into_iter()
        .map(|c| SummaryDef {
            name: c.name,
            measurement: c.measurement,
            aggregation: c.aggregation,
            filter: c.filter,
            bisect: c.bisect,
        })
        .collect();

    Ok(ExperimentDef {
        name: experiment.name,
        description: experiment.description,
        steps,
        summaries,
    })
}

fn toml_to_json(v: toml::Value) -> Result<serde_json::Value> {
    Ok(match v {
        toml::Value::String(s) => serde_json::Value::String(s),
        toml::Value::Integer(i) => serde_json::json!(i),
        toml::Value::Float(f) => serde_json::json!(f),
        toml::Value::Boolean(b) => serde_json::Value::Bool(b),
        toml::Value::Array(a) => serde_json::Value::Array(
            a.into_iter()
                .map(toml_to_json)
                .collect::<Result<Vec<_>>>()?,
        ),
        toml::Value::Table(t) => serde_json::Value::Object(
            t.into_iter()
                .map(|(k, v)| Ok((k, toml_to_json(v)?)))
                .collect::<Result<serde_json::Map<_, _>>>()?,
        ),
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
    })
}

// ── Git helpers ───────────────────────────────────────────────────────────────

pub mod git {
    use std::path::Path;
    use std::process::Command;

    use anyhow::{Context, Result, bail};

    pub fn current_sha(project_dir: &Path) -> Result<String> {
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(project_dir)
            .stderr(std::process::Stdio::null())
            .output()
            .context("running git rev-parse HEAD")?;
        if !out.status.success() {
            bail!("git rev-parse HEAD failed");
        }
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }

    pub fn upstream(project_dir: &Path) -> Result<String> {
        let out = Command::new("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(project_dir)
            .stderr(std::process::Stdio::null())
            .output()
            .context("running git remote get-url origin")?;
        if !out.status.success() {
            bail!("could not determine git remote origin");
        }
        let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(normalize_upstream(&raw))
    }

    pub fn commit_author(project_dir: &Path) -> String {
        let out = Command::new("git")
            .args(["log", "-1", "--format=%an"])
            .current_dir(project_dir)
            .output();
        out.ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_else(|| "unknown".to_string())
    }

    pub fn commit_message(project_dir: &Path) -> String {
        let out = Command::new("git")
            .args(["log", "-1", "--format=%s"])
            .current_dir(project_dir)
            .output();
        out.ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    }

    pub fn commit_timestamp(project_dir: &Path) -> String {
        let out = Command::new("git")
            .args(["log", "-1", "--format=%aI"])
            .current_dir(project_dir)
            .output();
        out.ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .unwrap_or_default()
    }

    pub fn apply_patch(project_dir: &Path, patch: &Path) -> Result<()> {
        let status = Command::new("git")
            .args(["apply", &patch.to_string_lossy()])
            .current_dir(project_dir)
            .status()
            .context("running git apply")?;
        if !status.success() {
            bail!("git apply {} failed", patch.display());
        }
        Ok(())
    }

    pub fn reset_worktree(repo_dir: &Path) -> Result<()> {
        let status = Command::new("git")
            .args(["checkout", "."])
            .current_dir(repo_dir)
            .status()
            .context("running git checkout .")?;
        if !status.success() {
            bail!("git checkout . failed");
        }
        let status = Command::new("git")
            .args(["clean", "-fd"])
            .current_dir(repo_dir)
            .status()
            .context("running git clean -fd")?;
        if !status.success() {
            bail!("git clean -fd failed");
        }
        Ok(())
    }

    pub fn fetch(repo_dir: &Path) -> Result<()> {
        let status = Command::new("git")
            .args(["fetch", "--quiet", "origin"])
            .current_dir(repo_dir)
            .status()
            .context("running git fetch")?;
        if !status.success() {
            bail!("git fetch failed");
        }
        Ok(())
    }

    pub fn checkout_detached(repo_dir: &Path, sha: &str) -> Result<()> {
        let status = Command::new("git")
            .args(["checkout", "--detach", sha])
            .current_dir(repo_dir)
            .status()
            .context("running git checkout --detach")?;
        if !status.success() {
            bail!("git checkout --detach {} failed", sha);
        }
        Ok(())
    }

    fn normalize_upstream(url: &str) -> String {
        let s = url
            .trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("ssh://")
            .trim_start_matches("git://");
        let s = if let Some(rest) = s.strip_prefix("git@") {
            rest.replacen(':', "/", 1)
        } else {
            s.to_string()
        };
        s.trim_end_matches(".git").to_string()
    }
}

// ── Plugin helpers ────────────────────────────────────────────────────────────

/// Directory where fetched plugins are cached.
pub fn plugin_cache_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".wezel")
        .join("plugins")
}

/// Resolve a plugin binary. Only looks in the plugin cache dir.
fn resolve_cached_plugin(name: &str) -> Option<PathBuf> {
    let binary_name = format!("forager-{name}");
    let path = plugin_cache_dir().join(&binary_name);
    path.is_file().then_some(path)
}

/// Resolve a plugin: check cache, then fetch from the source declared in config.
pub fn resolve_plugin(
    name: &str,
    plugins: &HashMap<String, String>,
) -> std::result::Result<PathBuf, StepError> {
    let binary_name = format!("forager-{name}");

    // Check cache first.
    if let Some(path) = resolve_cached_plugin(name) {
        return Ok(path);
    }

    // Look up source in config.
    let Some(source) = plugins.get(name) else {
        return Err(StepError::PluginNotFound {
            binary: binary_name,
        });
    };

    // Fetch from source.
    let path = fetch_plugin(name, source).map_err(|e| match e {
        fetch::FetchError::NotAvailable { plugin, target } => StepError::PluginNotFound {
            binary: format!("{plugin} (not available for {target})"),
        },
        other => StepError::Other(other.into()),
    })?;

    Ok(path)
}

/// Fetch a plugin from a source URI and install it to the cache dir.
fn fetch_plugin(name: &str, source: &str) -> Result<PathBuf, fetch::FetchError> {
    let binary_name = format!("forager-{name}");

    if let Some(repo) = source.strip_prefix("github:") {
        log::info!("fetching plugin `{binary_name}` from github.com/{repo}");
        fetch_from_github(name, repo)
    } else {
        Err(fetch::FetchError::Other(anyhow::anyhow!(
            "unsupported plugin source: {source}"
        )))
    }
}

/// Fetch a plugin binary from a GitHub repo's releases.
fn fetch_from_github(name: &str, repo: &str) -> Result<PathBuf, fetch::FetchError> {
    let binary_name = format!("forager-{name}");
    let target = fetch::current_target().ok_or_else(|| fetch::FetchError::NotAvailable {
        plugin: binary_name.clone(),
        target: "unknown".into(),
    })?;

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();

    // Find latest release.
    let releases_url = format!("https://api.github.com/repos/{repo}/releases");
    let mut req = agent.get(&releases_url).query("per_page", "10");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            req = req.set("Authorization", &format!("Bearer {token}"));
        }
    }
    req = req.set("User-Agent", "wezel-cli");

    let releases: Vec<serde_json::Value> = req
        .call()
        .map_err(|e| fetch::FetchError::Other(anyhow::anyhow!("fetching releases: {e}")))?
        .into_json()
        .map_err(|e| fetch::FetchError::Other(e.into()))?;

    let release = releases
        .first()
        .ok_or_else(|| fetch::FetchError::Other(anyhow::anyhow!("no releases found in {repo}")))?;

    let gh_assets = release["assets"]
        .as_array()
        .ok_or_else(|| fetch::FetchError::Other(anyhow::anyhow!("release has no assets")))?;

    // Match archive by binary name (hyphen or underscore) and target triple.
    let binary_name_underscored = binary_name.replace('-', "_");
    let asset = gh_assets
        .iter()
        .find(|a| {
            let fname = a["name"].as_str().unwrap_or("");
            (fname.contains(&binary_name) || fname.contains(&binary_name_underscored))
                && fname.contains(target)
                && !fname.ends_with(".sha256")
        })
        .ok_or_else(|| fetch::FetchError::NotAvailable {
            plugin: binary_name.clone(),
            target: target.to_string(),
        })?;

    let download_url = asset["browser_download_url"]
        .as_str()
        .ok_or_else(|| fetch::FetchError::Other(anyhow::anyhow!("asset has no download URL")))?;

    let mut dl_req = agent.get(download_url).set("User-Agent", "wezel-cli");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            dl_req = dl_req.set("Authorization", &format!("Bearer {token}"));
        }
    }

    let resp = dl_req
        .call()
        .map_err(|e| fetch::FetchError::Other(anyhow::anyhow!("downloading {binary_name}: {e}")))?;

    let mut bytes = Vec::new();
    use std::io::Read;
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| fetch::FetchError::Other(e.into()))?;

    let dest_dir = plugin_cache_dir();
    let dest = dest_dir.join(&binary_name);

    fetch::extract_and_install(&bytes, &binary_name, &dest)?;
    log::info!("installed `{binary_name}` to {}", dest.display());
    Ok(dest)
}

#[derive(Debug, thiserror::Error)]
pub enum StepError {
    #[error("plugin `{binary}` not found — declare it in [plugins] in .wezel/config.toml")]
    PluginNotFound { binary: String },

    #[error("failed to spawn `{binary}`: {reason}")]
    SpawnFailed { binary: String, reason: String },

    #[error("step '{step}': `{binary}` exited with {status}")]
    PluginFailed {
        step: String,
        binary: String,
        status: std::process::ExitStatus,
    },

    #[error("step '{step}': `{binary}` did not write FORAGER_OUT")]
    NoOutput { step: String, binary: String },

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl StepError {
    pub fn is_hard(&self) -> bool {
        matches!(self, Self::PluginNotFound { .. } | Self::SpawnFailed { .. })
    }
}

pub fn invoke_forager(
    forager_name: &str,
    step_name: &str,
    inputs: &serde_json::Value,
    project_dir: &Path,
    plugins: &HashMap<String, String>,
) -> std::result::Result<Vec<wezel_types::ForagerPluginOutput>, StepError> {
    let binary_name = format!("forager-{forager_name}");
    let binary = resolve_plugin(forager_name, plugins)?;

    // Write inputs to a temp file.
    let inputs_id = uuid::Uuid::new_v4();
    let inputs_path = std::env::temp_dir().join(format!("forager-inputs-{inputs_id}.json"));
    let out_path = std::env::temp_dir().join(format!("forager-out-{inputs_id}.json"));

    std::fs::write(&inputs_path, serde_json::to_string(inputs).unwrap())
        .map_err(|e| StepError::Other(anyhow::anyhow!("writing FORAGER_INPUTS: {e}")))?;

    let status = Command::new(&binary)
        .env("FORAGER_INPUTS", &inputs_path)
        .env("FORAGER_OUT", &out_path)
        .env("FORAGER_STEP", step_name)
        .current_dir(project_dir)
        .status()
        .map_err(|e| StepError::SpawnFailed {
            binary: binary_name.clone(),
            reason: e.to_string(),
        })?;

    if !status.success() {
        return Err(StepError::PluginFailed {
            step: step_name.to_string(),
            binary: binary_name,
            status,
        });
    }

    let envelope_raw = std::fs::read_to_string(&out_path).map_err(|_| StepError::NoOutput {
        step: step_name.to_string(),
        binary: binary_name.clone(),
    })?;

    let envelope: ForagerPluginEnvelope = serde_json::from_str(&envelope_raw)
        .map_err(|e| StepError::Other(anyhow::anyhow!("parsing output from {binary_name}: {e}")))?;

    // Best-effort cleanup of temp files.
    let _ = std::fs::remove_file(&inputs_path);
    let _ = std::fs::remove_file(&out_path);

    Ok(envelope.measurements)
}
