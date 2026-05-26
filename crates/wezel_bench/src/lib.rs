pub mod daemon;
pub mod fetch;
pub mod lint;
pub mod lockfile;
pub mod new;
pub mod run;
pub mod standalone;
pub mod workspace;

pub use workspace::Workspace;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use indexmap::{IndexMap, IndexSet};
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
    /// Target triples the project locks tool binaries for. `wezel project
    /// tool sync` fetches and hashes the release asset for each entry so
    /// `wezel.lock` is platform-complete and stable across machines/CI.
    /// Populated by `wezel project init` with the host triple; users add
    /// more as needed. An `IndexSet` so duplicates don't reach the lockfile
    /// while declaration order is preserved.
    #[serde(default)]
    pub targets: IndexSet<String>,
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
/// Steps live under `[step.<tool>.<step_name>]`. The outer map is keyed by
/// tool name and the inner by step name; bodies are wrapped in
/// `toml::Spanned` so the parser can recover file-line order across reopened
/// tables.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(title = "Wezel experiment.toml")]
pub struct ExperimentToml {
    /// Human-readable description of what the experiment measures.
    pub description: Option<String>,
    /// `[step.<tool>.<step_name>]`. Two-level map; step names must be unique
    /// across tools (enforced in [`parse_experiment`]).
    #[schemars(with = "HashMap<String, HashMap<String, StepBody>>")]
    pub step: IndexMap<String, IndexMap<String, toml::Spanned<StepBody>>>,
}

/// Either a boolean (uses `<step.name>.patch`) or an explicit patch filename.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum DiffField {
    Bool(bool),
    Name(String),
}

/// Body of a `[step.<tool>.<step_name>]` table. The tool name is recovered
/// from the outer key, not a field; flatten-collected `rest` is forwarded to
/// the forager plugin via `FORAGER_INPUTS` as JSON.
#[derive(Debug, Deserialize, JsonSchema)]
pub struct StepBody {
    pub description: Option<String>,
    /// Apply a patch before running this step. `true` uses `<step name>.patch`; a string overrides the filename.
    #[serde(rename = "apply-diff")]
    #[schemars(rename = "apply-diff")]
    pub apply_diff: Option<DiffField>,
    /// Summaries emitted by this step, keyed by summary name.
    #[serde(default)]
    #[schemars(with = "HashMap<String, EmbeddedSummaryToml>")]
    pub summary: IndexMap<String, EmbeddedSummaryToml>,
    /// Forager-specific inputs (e.g. `cmd`/`env`/`cwd` for `exec`, `package` for `llvm-lines`).
    #[serde(flatten)]
    #[schemars(with = "HashMap<String, serde_json::Value>")]
    pub rest: IndexMap<String, toml::Value>,
}

/// Summary definition embedded under a step. The summary's `name` and `step`
/// fields are recovered from the map keys (`step.<step>.summary.<name>`).
#[derive(Debug, Deserialize, JsonSchema)]
pub struct EmbeddedSummaryToml {
    /// Outcome name (as emitted by the forager) to aggregate over.
    pub outcome: String,
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

/// Render the JSON Schema for `experiment.toml`. Internal helper used by
/// [`build_bundle`] to seed the schemars-derived base; the editor-facing
/// schema lives in `.wezel/schema.json` and includes per-tool subschemas.
fn experiment_schema() -> serde_json::Value {
    let schema = schemars::schema_for!(ExperimentToml);
    serde_json::to_value(schema).expect("schema serialization is infallible")
}

/// Build `.wezel/schema.json` — the experiment-level schema with one
/// per-tool subschema rooted at `properties.step.properties.<tool>`.
///
/// The base shape comes from [`experiment_schema`]; the schemars-derived
/// `step.additionalProperties` is replaced by explicit `step.properties`
/// (one entry per installed forager) with `additionalProperties: false`, so
/// unknown tool names get flagged in the editor. For each forager we emit a
/// `Step_<tool>` definition that combines `StepBody`'s common fields with
/// that forager's `inputs.properties`/`required` and layers
/// `outcomes_doc` onto `summary.<x>.outcome.description`.
pub fn build_bundle<I>(foragers: I) -> serde_json::Value
where
    I: IntoIterator<Item = ForagerSchema>,
{
    let mut bundle = experiment_schema();

    let foragers: Vec<ForagerSchema> = foragers.into_iter().collect();
    if foragers.is_empty() {
        return bundle;
    }

    // Pull StepBody out of the definitions so each per-tool def can copy its
    // common fields (description, apply-diff, summary).
    let common = bundle
        .pointer("/definitions/StepBody")
        .cloned()
        .expect("schemars-derived schema must define StepBody");

    let mut step_properties = serde_json::Map::new();
    for forager in &foragers {
        let def_name = format!("Step_{}", forager.name);
        let def = build_step_def(&common, forager);
        if let Some(defs) = bundle
            .pointer_mut("/definitions")
            .and_then(|v| v.as_object_mut())
        {
            defs.insert(def_name.clone(), def);
        }
        step_properties.insert(
            forager.name.clone(),
            serde_json::json!({
                "type": "object",
                "description": forager.description,
                "additionalProperties": {
                    "$ref": format!("#/definitions/{def_name}")
                }
            }),
        );
    }

    if let Some(step) = bundle
        .pointer_mut("/properties/step")
        .and_then(|v| v.as_object_mut())
    {
        // Replace the schemars-derived open `additionalProperties` with the
        // explicit per-tool properties + a closed door for unknown tools.
        step.remove("additionalProperties");
        step.insert(
            "additionalProperties".into(),
            serde_json::Value::Bool(false),
        );
        step.insert(
            "properties".into(),
            serde_json::Value::Object(step_properties),
        );
    }

    // StepBody has been inlined into every Step_<tool>; nobody references the
    // bare definition anymore.
    if let Some(defs) = bundle
        .pointer_mut("/definitions")
        .and_then(|v| v.as_object_mut())
    {
        defs.remove("StepBody");
    }

    bundle
}

fn build_step_def(common: &serde_json::Value, forager: &ForagerSchema) -> serde_json::Value {
    let mut def = common.clone();
    let obj = def
        .as_object_mut()
        .expect("StepBody definition must be an object");

    // Lock the schema down — anything that isn't a common field or a known
    // forager input should surface as an editor diagnostic.
    obj.insert(
        "additionalProperties".into(),
        serde_json::Value::Bool(false),
    );

    // Merge the forager's input properties into the common ones.
    let props = obj
        .entry("properties")
        .or_insert_with(|| serde_json::json!({}))
        .as_object_mut()
        .expect("properties must be an object");
    if let Some(input_props) = forager.inputs.get("properties").and_then(|v| v.as_object()) {
        for (k, v) in input_props {
            props.insert(k.clone(), v.clone());
        }
    }

    // Layer the forager's outcomes_doc onto each summary's outcome
    // description so editors surface per-tool hints on hover.
    if let Some(summary_schema) = props.get_mut("summary").and_then(|v| v.as_object_mut())
        && let Some(additional) = summary_schema.get_mut("additionalProperties")
    {
        *additional = serde_json::json!({
            "allOf": [
                additional.clone(),
                {
                    "properties": {
                        "outcome": {
                            "type": "string",
                            "description": forager.outcomes_doc,
                        }
                    }
                }
            ]
        });
    }

    // Forward the forager's `required` fields onto the step def.
    if let Some(required) = forager.inputs.get("required").and_then(|v| v.as_array()) {
        let existing = obj
            .entry("required")
            .or_insert_with(|| serde_json::Value::Array(Vec::new()))
            .as_array_mut()
            .expect("required must be an array");
        for r in required {
            if !existing.contains(r) {
                existing.push(r.clone());
            }
        }
    }

    def
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

    // Flatten the two-level `step.<tool>.<name>` map and recover source order
    // from each body's span. TOML merges all entries for an outer key into a
    // single table, so cross-tool ordering is otherwise lost.
    let mut entries: Vec<(String, String, usize, StepBody)> = Vec::new();
    for (tool, steps_for_tool) in experiment.step {
        for (step_name, spanned_body) in steps_for_tool {
            let span_start = spanned_body.span().start;
            entries.push((
                tool.clone(),
                step_name,
                span_start,
                spanned_body.into_inner(),
            ));
        }
    }
    entries.sort_by_key(|(_, _, span, _)| *span);

    // Step names must be unique across tools — patch filenames and the
    // `measurements.step` column in burrow assume globally-unique names.
    let mut seen: HashSet<String> = HashSet::new();
    for (_, name, _, _) in &entries {
        if !seen.insert(name.clone()) {
            bail!(
                "step name `{name}` is declared under more than one tool; step names must be unique"
            );
        }
    }

    let mut steps = Vec::with_capacity(entries.len());
    let mut summaries = Vec::new();
    for (forager, step_name, _, body) in entries {
        let inputs_map: serde_json::Map<String, serde_json::Value> = body
            .rest
            .into_iter()
            .map(|(k, v)| Ok((k, toml_to_json(v)?)))
            .collect::<Result<_>>()?;

        let diff = match body.apply_diff {
            Some(DiffField::Bool(true)) => Some(step_name.clone()),
            Some(DiffField::Bool(false)) | None => None,
            Some(DiffField::Name(s)) => Some(s),
        };

        for (summary_name, s) in body.summary {
            summaries.push(SummaryDef {
                name: summary_name,
                step: step_name.clone(),
                measurement: s.outcome,
                aggregation: s.aggregation,
                filter: s.filter,
                bisect: s.bisect,
                samples: s.samples,
            });
        }

        steps.push(StepDef {
            name: step_name,
            forager,
            description: body.description,
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

    /// Step + summary order must follow source order, not the tool-grouped
    /// order that the deserialised two-level map would naturally produce.
    /// The steps below interleave tools (cargo → exec → cargo) so a parser
    /// that iterates `step.<tool>` in map order would visibly fail.
    #[test]
    fn preserves_source_order() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("experiment.toml"),
            r#"
description = "ordering check"

[step.cargo.zzz-first]
command = "build"
build_target = "workspace"
summary.zzz-sum = { outcome = "time_ms" }
summary.aaa-sum = { outcome = "time_ms" }

[step.exec.middle]
cmd = "true"

[step.cargo.aaa-third]
command = "build"
build_target = "workspace"
"#,
        )
        .unwrap();

        let exp = parse_experiment(tmp.path()).unwrap();
        let step_names: Vec<_> = exp.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(step_names, vec!["zzz-first", "middle", "aaa-third"]);

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

    /// Step names must be unique across tools — same name under two
    /// different tools collides with patch filenames and burrow's
    /// `measurements.step` column.
    #[test]
    fn rejects_step_name_collision_across_tools() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("experiment.toml"),
            r#"
[step.exec.baseline]
cmd = "true"

[step.cargo.baseline]
command = "build"
build_target = "workspace"
"#,
        )
        .unwrap();

        let err = parse_experiment(tmp.path()).unwrap_err().to_string();
        assert!(
            err.contains("baseline") && err.contains("more than one tool"),
            "expected collision error mentioning the step name, got: {err}"
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

    /// Current branch name, or `None` when HEAD is detached.
    pub fn current_branch(project_dir: &Path) -> Result<Option<String>> {
        let out = Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(project_dir)
            .stderr(std::process::Stdio::null())
            .output()
            .context("running git rev-parse --abbrev-ref HEAD")?;
        if !out.status.success() {
            bail!("git rev-parse --abbrev-ref HEAD failed");
        }
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        Ok(if name == "HEAD" || name.is_empty() {
            None
        } else {
            Some(name)
        })
    }

    /// Tracked files modified or staged but not committed. Untracked files are
    /// ignored (they aren't in HEAD and can't affect a measurement taken at HEAD).
    pub fn is_dirty(project_dir: &Path) -> Result<bool> {
        let out = Command::new("git")
            .args(["status", "--porcelain", "--untracked-files=no"])
            .current_dir(project_dir)
            .stderr(std::process::Stdio::null())
            .output()
            .context("running git status --porcelain")?;
        if !out.status.success() {
            bail!("git status failed");
        }
        Ok(!out.stdout.is_empty())
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

    Ok(envelope.outcomes)
}
