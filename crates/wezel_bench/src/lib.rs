pub mod daemon;
pub mod fetch;
pub mod lint;
pub mod new;
pub mod run;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use figment::Figment;
use figment::providers::{Format, Serialized, Toml};
use serde::{Deserialize, Serialize};
use wezel_types::ForagerPluginEnvelope;

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProjectConfig {
    pub server_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub server_url: String,
}

impl Config {
    pub fn load(project_dir: &Path) -> Result<Config> {
        let config_path = project_dir.join(".wezel").join("config.toml");
        if !config_path.is_file() {
            bail!("no .wezel/config.toml found at {}", config_path.display());
        }
        let defaults = ProjectConfig { server_url: None };
        let resolved: ProjectConfig = Figment::new()
            .merge(Serialized::defaults(defaults))
            .merge(Toml::file(&config_path))
            .extract()
            .with_context(|| format!("loading config from {}", config_path.display()))?;
        let server_url = resolved
            .server_url
            .filter(|s| !s.is_empty())
            .with_context(|| format!("server_url not set in {}", config_path.display()))?;
        Ok(Config { server_url })
    }
}

// ── Experiment TOML parsing ──────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub(crate) struct ExperimentToml {
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<ExperimentStepToml>,
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

pub struct ParsedStep {
    pub name: String,
    pub forager: String,
    #[allow(dead_code)]
    pub description: Option<String>,
    /// Resolved patch stem: `Some("add-one-fn")` means `add-one-fn.patch` must exist.
    pub diff: Option<String>,
    pub inputs: serde_json::Value,
}

pub fn parse_experiment(experiment_dir: &Path) -> Result<(String, Option<String>, Vec<ParsedStep>)> {
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

        steps.push(ParsedStep {
            name: raw_step.name,
            forager,
            description: raw_step.description,
            diff,
            inputs: serde_json::Value::Object(inputs_map),
        });
    }

    Ok((experiment.name, experiment.description, steps))
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

pub fn resolve_plugin(name: &str) -> Option<PathBuf> {
    let binary_name = format!("forager-{name}");
    std::env::current_exe()
        .ok()
        .and_then(|exe| {
            let sibling = exe.parent()?.join(&binary_name);
            sibling.is_file().then_some(sibling)
        })
        .or_else(|| which::which(&binary_name).ok())
}

#[derive(Debug, thiserror::Error)]
pub enum StepError {
    #[error("`{binary}` not found on PATH — is it installed?")]
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
    fetcher: Option<&dyn fetch::PluginFetcher>,
) -> std::result::Result<Option<wezel_types::ForagerPluginOutput>, StepError> {
    let binary_name = format!("forager-{forager_name}");
    // Look next to our own executable first, then fall back to PATH.
    // If not found and a fetcher is available, try to download and install.
    let binary = match resolve_plugin(forager_name) {
        Some(path) => path,
        None => match fetcher {
            Some(f) => f.fetch(forager_name).map_err(|e| match e {
                fetch::FetchError::Declined { .. } => StepError::PluginNotFound {
                    binary: binary_name.clone(),
                },
                other => StepError::Other(other.into()),
            })?,
            None => {
                return Err(StepError::PluginNotFound {
                    binary: binary_name.clone(),
                });
            }
        },
    };

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

    Ok(envelope.measurement)
}
