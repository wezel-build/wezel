pub mod daemon;
pub mod fetch;
pub mod lint;
pub mod lockfile;
pub mod new;
pub mod run;
pub mod standalone;
pub mod workspace;

pub use workspace::Workspace;

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use wezel_types::{
    Aggregation, ExperimentDef, ForagerPluginEnvelope, ForagerSchema, StepDef, SummaryDef,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project_id: uuid::Uuid,
    pub name: String,
    #[serde(default = "StorageTarget::default_target")]
    pub target: StorageTarget,
    /// External tool sources declared under `[tools]` in `.wezel/config.toml`.
    #[serde(default)]
    pub tools: ToolsSection,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageTarget {
    ServerUrl(String),
    DataBranch(String),
}

impl StorageTarget {
    fn default_target() -> StorageTarget {
        StorageTarget::DataBranch("wezel/data".to_owned())
    }
    pub fn server_url(&self) -> Option<&str> {
        if let Self::ServerUrl(url) = &self {
            Some(url)
        } else {
            None
        }
    }
    pub fn data_branch(&self) -> Option<&str> {
        if let Self::DataBranch(branch) = &self {
            Some(branch)
        } else {
            None
        }
    }
}

/// Umbrella for declared external binaries — foragers today, with room for
/// pheromones, explainers, etc. as their installs become first-class.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ToolsSection {
    /// Map of forager name → install source. Keys correspond to the `tool`
    /// field of an experiment step (e.g. `tool = "exec"` looks up
    /// `[tools.foragers.exec]`).
    #[serde(default)]
    pub foragers: BTreeMap<String, ToolSource>,
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

impl ProjectConfig {
    pub fn load(project_dir: &Path) -> Result<ProjectConfig> {
        let config_path = project_dir.join(".wezel").join("config.toml");
        if !config_path.is_file() {
            bail!("no .wezel/config.toml found at {}", config_path.display());
        }
        let raw = std::fs::read_to_string(&config_path)
            .with_context(|| format!("reading {}", config_path.display()))?;
        let resolved: ProjectConfig = toml::from_str(&raw)
            .with_context(|| format!("Failed parsing {}", config_path.display()))?;
        // server_url: env var takes precedence, then config file.
        let target = std::env::var("WEZEL_BURROW_URL")
            .ok()
            .and_then(|s| (!s.is_empty()).then_some(StorageTarget::ServerUrl(s)))
            .unwrap_or(resolved.target);
        Ok(ProjectConfig {
            project_id: resolved.project_id,
            name: resolved.name,
            target,
            tools: resolved.tools,
        })
    }
}

// ── Experiment TOML parsing ──────────────────────────────────────────────────

/// Top-level shape of `.wezel/experiments/<name>/experiment.toml`.
///
/// Steps are a map keyed by step name; insertion order is preserved by the
/// `preserve_order` feature on the `toml` crate plus `IndexMap` here.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(title = "Wezel experiment.toml")]
pub struct ExperimentToml {
    /// Human-readable description of what the experiment measures.
    pub description: Option<String>,
    /// Ordered map of step name → step definition. Patches are cumulative.
    #[schemars(with = "HashMap<String, ExperimentStepToml>")]
    pub step: IndexMap<String, ExperimentStepToml>,
}

/// Either a boolean (uses `<step.name>.patch`) or an explicit patch filename.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum DiffField {
    Bool(bool),
    Name(String),
}

/// A single step in the experiment. The `tool` field selects a forager plugin;
/// remaining fields are passed to the plugin via `FORAGER_INPUTS` as JSON.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExperimentStepToml {
    /// Forager plugin name (resolves to `forager-<tool>`). Required.
    pub tool: String,
    pub description: Option<String>,
    /// Apply a patch before running this step. `true` uses `<step name>.patch`; a string overrides the filename.
    #[serde(rename = "apply-diff")]
    #[schemars(rename = "apply-diff")]
    pub apply_diff: Option<DiffField>,
    /// Summaries emitted by this step, keyed by summary name.
    #[serde(default)]
    #[schemars(with = "HashMap<String, EmbeddedSummaryToml>")]
    pub summary: IndexMap<String, EmbeddedSummaryToml>,
    /// Remaining fields are forwarded as forager inputs (e.g. `cmd`/`env`/`cwd` for `exec`, `package` for `llvm-lines`).
    #[serde(flatten)]
    #[schemars(with = "HashMap<String, serde_json::Value>")]
    pub rest: IndexMap<String, toml::Value>,
}

/// Summary definition embedded under a step. The summary's `name` and `step`
/// fields are recovered from the map keys (`step.<step>.summary.<name>`).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EmbeddedSummaryToml {
    /// Measurement name (as emitted by the forager) to aggregate over.
    pub measurement: String,
    /// How to combine multiple matching values. Omit when the filter is
    /// expected to select a single value.
    #[serde(default)]
    pub aggregation: Option<Aggregation>,
    /// Tag key=value filters applied before aggregation.
    #[serde(default)]
    #[schemars(with = "HashMap<String, String>")]
    pub filter: IndexMap<String, String>,
    /// Trigger bisection when this summary regresses.
    #[serde(default = "bool_true")]
    pub bisect: bool,
    /// Number of forager invocations of the step to take. Lint requires all
    /// summaries on the same step to agree. Default 1.
    #[serde(default = "one_usize")]
    pub samples: usize,
}

fn one_usize() -> usize {
    1
}

/// Render the JSON Schema for `experiment.toml`.
pub fn experiment_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(ExperimentToml);
    serde_json::to_value(schema).expect("schema serialization is infallible")
}

/// Build `.wezel/schema.json` — the experiment-level schema with one
/// `if`/`then` branch per installed forager.
///
/// The base shape comes from [`experiment_schema`]; for each forager we add a
/// branch on `$defs/ExperimentStepToml` that activates when `tool == <name>`,
/// splicing in the forager's flat input properties and overriding the
/// `description` of `summary.<x>.measurement` with the forager's
/// `measurements_doc`. Editors keyed off the `tool` discriminator surface
/// per-forager hints throughout the step.
pub fn build_bundle<I>(foragers: I) -> serde_json::Value
where
    I: IntoIterator<Item = ForagerSchema>,
{
    let mut bundle = experiment_schema();
    let Some(step_def) = bundle
        .pointer_mut("/definitions/ExperimentStepToml")
        .and_then(|v| v.as_object_mut())
    else {
        // experiment_schema is generated from a stable type; this is a
        // programming error if it ever fires.
        panic!("experiment schema missing definitions/ExperimentStepToml");
    };

    let mut branches = Vec::new();
    for forager in foragers {
        branches.push(forager_branch(&forager));
    }

    if !branches.is_empty() {
        step_def.insert("allOf".into(), serde_json::Value::Array(branches));
    }
    bundle
}

fn forager_branch(forager: &ForagerSchema) -> serde_json::Value {
    let mut then_props = serde_json::Map::new();

    // Splice the forager's input properties as flat keys of the step. They
    // live on the step directly because ExperimentStepToml uses serde(flatten)
    // for forager inputs.
    if let Some(input_props) = forager.inputs.get("properties").and_then(|v| v.as_object()) {
        for (k, v) in input_props {
            then_props.insert(k.clone(), v.clone());
        }
    }

    // Override summary entries' measurement.description with this forager's
    // documentation. We layer the override on top of the base summary schema
    // via allOf so the existing `aggregation`/`filter`/etc. constraints are
    // preserved.
    then_props.insert(
        "summary".into(),
        serde_json::json!({
            "type": "object",
            "additionalProperties": {
                "allOf": [
                    { "$ref": "#/definitions/EmbeddedSummaryToml" },
                    {
                        "properties": {
                            "measurement": {
                                "description": forager.measurements_doc,
                            }
                        }
                    }
                ]
            }
        }),
    );

    let mut then = serde_json::Map::new();
    then.insert("properties".into(), serde_json::Value::Object(then_props));
    if let Some(required) = forager.inputs.get("required").cloned() {
        then.insert("required".into(), required);
    }

    serde_json::json!({
        "if": { "properties": { "tool": { "const": forager.name } } },
        "then": serde_json::Value::Object(then),
    })
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

    let mut steps = Vec::with_capacity(experiment.step.len());
    let mut summaries = Vec::new();
    for (step_name, raw_step) in experiment.step {
        let forager = raw_step.tool;

        let inputs_map: serde_json::Map<String, serde_json::Value> = raw_step
            .rest
            .into_iter()
            .map(|(k, v)| Ok((k, toml_to_json(v)?)))
            .collect::<Result<_>>()?;

        let diff = match raw_step.apply_diff {
            Some(DiffField::Bool(true)) => Some(step_name.clone()),
            Some(DiffField::Bool(false)) | None => None,
            Some(DiffField::Name(s)) => Some(s),
        };

        for (summary_name, s) in raw_step.summary {
            summaries.push(SummaryDef {
                name: summary_name,
                step: step_name.clone(),
                measurement: s.measurement,
                aggregation: s.aggregation,
                filter: s.filter,
                bisect: s.bisect,
                samples: s.samples,
            });
        }

        steps.push(StepDef {
            name: step_name,
            forager,
            description: raw_step.description,
            diff,
            inputs: serde_json::Value::Object(inputs_map),
        });
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    /// Step + summary order must follow source order, not alphabetical. The
    /// step names below are reverse-alphabetical so a BTreeMap-backed parser
    /// would visibly fail this test.
    #[test]
    fn preserves_source_order() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("experiment.toml"),
            r#"
description = "ordering check"

[step.zzz-first]
tool = "exec"
cmd = "true"
summary.zzz-sum = { measurement = "m1" }
summary.aaa-sum = { measurement = "m2" }

[step.aaa-second]
tool = "exec"
cmd = "true"
"#,
        )
        .unwrap();

        let exp = parse_experiment(tmp.path()).unwrap();
        let step_names: Vec<_> = exp.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(step_names, vec!["zzz-first", "aaa-second"]);

        let summary_keys: Vec<_> = exp
            .summaries
            .iter()
            .map(|s| (s.step.as_str(), s.name.as_str()))
            .collect();
        assert_eq!(
            summary_keys,
            vec![("zzz-first", "zzz-sum"), ("zzz-first", "aaa-sum")]
        );
    }
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

#[derive(Debug, thiserror::Error)]
pub enum StepError {
    #[error("`{binary}` not found in the local store — is it installed?")]
    PluginNotFound { binary: String },

    #[error("failed to spawn `{binary}`: {reason}")]
    SpawnFailed { binary: String, reason: String },

    #[error("step '{step}': `{binary}` exited with {status}{}", fmt_captured(.stdout, .stderr))]
    PluginFailed {
        step: String,
        binary: String,
        status: std::process::ExitStatus,
        stdout: String,
        stderr: String,
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

fn fmt_captured(stdout: &str, stderr: &str) -> String {
    let mut s = String::new();
    let stderr = stderr.trim();
    let stdout = stdout.trim();
    if !stderr.is_empty() {
        s.push_str("\n--- stderr ---\n");
        s.push_str(stderr);
    }
    if !stdout.is_empty() {
        s.push_str("\n--- stdout ---\n");
        s.push_str(stdout);
    }
    s
}

pub fn invoke_forager(
    forager_name: &str,
    step_name: &str,
    inputs: &serde_json::Value,
    workspace: &Workspace,
    fetcher: Option<&mut (dyn fetch::PluginFetcher + '_)>,
) -> std::result::Result<Vec<wezel_types::ForagerPluginOutput>, StepError> {
    let binary_name = format!("forager-{forager_name}");
    // Resolve from the local store; if missing, ask the fetcher to install.
    let binary = match workspace.resolve_plugin(forager_name) {
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

    let output = Command::new(&binary)
        .env("FORAGER_INPUTS", &inputs_path)
        .env("FORAGER_OUT", &out_path)
        .env("FORAGER_STEP", step_name)
        .current_dir(&workspace.project_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .map_err(|e| StepError::SpawnFailed {
            binary: binary_name.clone(),
            reason: e.to_string(),
        })?;

    if !output.status.success() {
        return Err(StepError::PluginFailed {
            step: step_name.to_string(),
            binary: binary_name,
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
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
