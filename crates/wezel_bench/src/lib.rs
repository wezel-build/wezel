pub mod daemon;
pub mod fetch;
pub mod lint;
pub mod lockfile;
pub mod new;
pub mod run;
pub mod standalone;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use schemars::JsonSchema;
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
    pub tools: ToolsSection,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
    pub project_id: uuid::Uuid,
    pub name: String,
    pub server_url: Option<String>,
    /// Branch used for standalone state storage (default: "wezel/data").
    pub data_branch: String,
    /// External tool sources declared under `[tools]` in `.wezel/config.toml`.
    pub tools: ToolsSection,
}

/// Umbrella for declared external binaries — foragers today, with room for
/// pheromones, explainers, etc. as their installs become first-class.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ToolsSection {
    /// Map of forager name → install source. Keys correspond to the `tool`
    /// field of an experiment step (e.g. `tool = "exec"` looks up
    /// `[tools.foragers.exec]`).
    #[serde(default)]
    pub foragers: HashMap<String, ToolSource>,
}

/// Where to obtain a tool binary. Currently only GitHub releases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSource {
    /// `owner/repo` on github.com.
    pub github: String,
    /// Optional release tag pin. Default: latest release.
    #[serde(default)]
    pub tag: Option<String>,
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
            tools: resolved.tools,
        })
    }
}

// ── Experiment TOML parsing ──────────────────────────────────────────────────

/// Top-level shape of `.wezel/experiments/<name>/experiment.toml`.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(title = "Wezel experiment.toml")]
pub struct ExperimentToml {
    /// Human-readable description of what the experiment measures.
    pub description: Option<String>,
    /// Ordered list of forager steps. Patches are cumulative across steps.
    pub steps: Vec<ExperimentStepToml>,
    /// Named scalars derived from measurements, used for regression detection.
    #[serde(default)]
    pub summaries: Vec<SummaryToml>,
}

/// Either a boolean (uses `<step.name>.patch`) or an explicit patch filename.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum DiffField {
    Bool(bool),
    Name(String),
}

fn default_exec() -> String {
    "exec".to_owned()
}
/// A single step in the experiment. The `tool` field selects a forager plugin;
/// remaining fields are passed to the plugin via `FORAGER_INPUTS` as JSON.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExperimentStepToml {
    /// Step identifier. Also the default patch filename stem when `apply-diff = true`.
    pub name: String,
    /// Forager plugin name (resolves to `forager-<tool>`). Defaults to `exec` if `cmd` is set.
    #[serde(default = "default_exec")]
    pub tool: String,
    pub description: Option<String>,
    /// Apply a patch before running this step. `true` uses `<name>.patch`; a string overrides the filename.
    #[serde(rename = "apply-diff")]
    #[schemars(rename = "apply-diff")]
    pub apply_diff: Option<DiffField>,
    /// Remaining fields are forwarded as forager inputs (e.g. `cmd`/`env`/`cwd` for `exec`, `package` for `llvm-lines`).
    #[serde(flatten)]
    #[schemars(with = "HashMap<String, serde_json::Value>")]
    pub rest: HashMap<String, toml::Value>,
}

/// Aggregates a set of measurements into a single scalar tracked for regressions.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct SummaryToml {
    /// Summary identifier; surfaced in the dashboard.
    pub name: String,
    /// Measurement name (as emitted by the forager) to aggregate over.
    pub measurement: String,
    /// How to combine multiple matching values. Omit when the filter is
    /// expected to select a single value.
    #[serde(default)]
    pub aggregation: Option<Aggregation>,
    /// Tag key=value filters applied before aggregation.
    #[serde(default)]
    pub filter: HashMap<String, String>,
    /// Trigger bisection when this summary regresses.
    #[serde(default = "bool_true")]
    pub bisect: bool,
}

/// Render the JSON Schema for `experiment.toml`.
pub fn experiment_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(ExperimentToml);
    serde_json::to_value(schema).expect("schema serialization is infallible")
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
        let forager = raw_step.tool;

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
        name: experiment_dir
            .file_name()
            .context("Could not extract dir name from experiment directory")?
            .to_str()
            .context("Expected experiment name to be valid UTF-8")?
            .to_owned(),
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

/// Resolve a forager binary in the local store only.
///
/// The store is the directory next to the wezel binary, or the path in
/// `WEZEL_PLUGIN_DIR` when set (used by tests). PATH is never consulted.
pub fn resolve_plugin(name: &str) -> Option<PathBuf> {
    let binary_name = format!("forager-{name}");
    let candidate = fetch::plugin_install_dir()?.join(&binary_name);
    candidate.is_file().then_some(candidate)
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
    fetcher: Option<&mut (dyn fetch::PluginFetcher + '_)>,
) -> std::result::Result<Vec<wezel_types::ForagerPluginOutput>, StepError> {
    let binary_name = format!("forager-{forager_name}");
    // Resolve from the local store; if missing, ask the fetcher to install.
    let binary = match resolve_plugin(forager_name) {
        Some(path) => path,
        None => match fetcher {
            Some(f) => f
                .fetch(forager_name)
                .map_err(|e| StepError::Other(e.into()))?,
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

    Ok(envelope.measurements)
}
