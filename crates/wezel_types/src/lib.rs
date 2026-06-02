//! Shared types for the wezel ecosystem.
//!
//! These mirror the data model consumed by the Anthill frontend.

use std::collections::HashMap;

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ── Project ──────────────────────────────────────────────────────────────────

/// A project, identified across machines by its upstream (git remote) URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: u64,
    /// Human-readable name, e.g. "wezel" (derived from the repo name).
    pub name: String,
    /// Canonical upstream URL (e.g. "github.com/wezel-build/wezel").
    /// Stripped of protocol and `.git` suffix so that SSH and HTTPS
    /// remotes resolve to the same identity.
    pub upstream: String,
}

// ── Dependency graph ─────────────────────────────────────────────────────────

/// A crate and its direct dependencies, forming one node in the build graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateTopo {
    pub name: String,
    /// Semver version string, e.g. "1.2.3".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// Normal (runtime) dependencies — primary structural edges.
    pub deps: Vec<String>,
    /// Build-script dependencies (`[build-dependencies]`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub build_deps: Vec<String>,
    /// Dev-only dependencies (`[dev-dependencies]`); excluded from layout to avoid cycles.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dev_deps: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
}

// ── Build runs ───────────────────────────────────────────────────────────────

/// A single observed build invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub user: String,
    pub platform: String,
    pub timestamp: String,
    pub commit: String,
    pub build_time_ms: u64,
    pub dirty_crates: Vec<String>,
}

// ── Observations ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Dev,
    Release,
}

/// An observed build: a specific project + profile combination being tracked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub id: u64,
    pub name: String,
    pub profile: Profile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    pub pinned: bool,
    pub graph: Vec<CrateTopo>,
    pub runs: Vec<Run>,
}

// ── Forager / commit metrics ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MeasurementStatus {
    NotStarted,
    Pending,
    Running,
    Complete,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    pub id: u64,
    pub name: String,
    pub status: MeasurementStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub tags: IndexMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CommitStatus {
    NotStarted,
    Running,
    Complete,
}

/// A commit with associated forager measurements.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForagerCommit {
    pub sha: String,
    pub short_sha: String,
    pub author: String,
    pub message: String,
    pub timestamp: String,
    pub measurements: Vec<Measurement>,
}

// ── Bisection ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BisectionStatus {
    Active,
    Complete,
    Abandoned,
}

/// A bisection tracking regression between two commits.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bisection {
    pub id: u64,
    pub project_id: u64,
    pub experiment_name: String,
    pub measurement_name: String,
    pub branch: String,
    pub good_sha: String,
    pub bad_sha: String,
    pub good_value: f64,
    pub bad_value: f64,
    pub status: BisectionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub culprit_sha: Option<String>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub identity_tags: IndexMap<String, String>,
}

// ── Forager runner types ─────────────────────────────────────────────────────

/// Experiment definition parsed from `.wezel/experiments/<name>/experiment.toml`.
#[derive(Debug, Clone, Serialize)]
pub struct ExperimentDef {
    pub name: String,
    pub description: Option<String>,
    pub steps: Vec<StepDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub summaries: Vec<SummaryDef>,
}

/// A single step in an experiment.
#[derive(Debug, Clone, Serialize)]
pub struct StepDef {
    /// Step identifier; also used as the default patch filename stem.
    pub name: String,
    /// Resolved forager name (e.g. `"exec"`, `"llvm-lines"`).
    pub forager: String,
    pub description: Option<String>,
    /// Explicit patch filename override; `None` = use `<name>.patch` convention.
    pub diff: Option<String>,
    /// Forager-specific inputs serialised from the remaining TOML fields.
    pub inputs: serde_json::Value,
}

/// How to aggregate measurement values into a conclusion scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
#[schemars(rename_all = "kebab-case")]
pub enum Aggregation {
    Sum,
    Mean,
    Median,
    Max,
    Min,
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

/// A named scalar derived from measurements, used for regression detection.
///
/// Defined in `.wezel/experiments/<name>/experiment.toml` under `[[summaries]]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryDef {
    pub name: String,
    /// Step the measurement was emitted by.
    pub step: String,
    /// Measurement name to aggregate over.
    pub measurement: String,
    /// How to combine multiple matching values. Omit when the filter is
    /// expected to select a single value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aggregation: Option<Aggregation>,
    /// Tag key=value filters applied before aggregation.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub filter: IndexMap<String, String>,
    /// Whether to trigger bisection on regression (default true).
    #[serde(default = "bool_true")]
    pub bisect: bool,
    /// Number of forager invocations of `step` to perform. The runner takes
    /// one snapshot before iter 1 and restores it before each later iter, so
    /// samples are i.i.d. Lint enforces a single value per step (across all
    /// summaries that reference it). Default 1.
    #[serde(default = "one")]
    pub samples: usize,
}

fn one() -> usize {
    1
}

fn bool_true() -> bool {
    true
}

/// Reasons `SummaryDef::compute` may fail. Distinct from "no matches", which is
/// `Ok(None)`.
#[derive(Debug, Clone)]
pub enum SummaryError {
    /// More than one value matched the filter, but no `aggregation` was set.
    AmbiguousAggregation { matches: usize },
}

impl std::fmt::Display for SummaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AmbiguousAggregation { matches } => write!(
                f,
                "{matches} values matched the filter but no `aggregation` was specified"
            ),
        }
    }
}

impl std::error::Error for SummaryError {}

impl SummaryDef {
    /// Pre-aggregation values matched by `step` + `measurement` + `filter`.
    /// Callers wanting distribution data (n, min, max) alongside the scalar
    /// can take it from here; `compute` reduces this to a single value.
    pub fn matching_values(&self, steps: &[ExperimentRunStep]) -> Vec<f64> {
        steps
            .iter()
            .filter(|s| s.step == self.step)
            .flat_map(|s| &s.measurements)
            .filter(|m| m.name == self.measurement)
            .filter(|m| {
                self.filter
                    .iter()
                    .all(|(k, v)| m.tags.get(k).map(|s| s.as_str()) == Some(v.as_str()))
            })
            .filter_map(|m| m.value.as_f64())
            .collect()
    }

    /// Compute this summary's value from a slice of plugin measurements.
    ///
    /// Returns `Ok(None)` when no measurements match the filter. Returns
    /// `Err(AmbiguousAggregation)` when multiple values match but the summary
    /// did not specify how to combine them.
    pub fn compute(&self, steps: &[ExperimentRunStep]) -> Result<Option<f64>, SummaryError> {
        let mut values = self.matching_values(steps);

        if values.is_empty() {
            return Ok(None);
        }

        let aggregation = match self.aggregation {
            Some(a) => a,
            None => {
                if values.len() == 1 {
                    return Ok(Some(values[0]));
                }
                return Err(SummaryError::AmbiguousAggregation {
                    matches: values.len(),
                });
            }
        };

        Ok(Some(match aggregation {
            Aggregation::Sum => values.iter().sum(),
            Aggregation::Mean => values.iter().sum::<f64>() / values.len() as f64,
            Aggregation::Max => values.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
            Aggregation::Min => values.iter().cloned().fold(f64::INFINITY, f64::min),
            Aggregation::Median => {
                values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                let mid = values.len() / 2;
                if values.len().is_multiple_of(2) {
                    (values[mid - 1] + values[mid]) / 2.0
                } else {
                    values[mid]
                }
            }
        }))
    }
}

/// An experiment run row claimed by wezel-cli via `POST /api/runs/claim`.
/// Authentication is via the caller's `wez_live_…` API token (Authorization
/// header); `id` is the row id, used as the round-trip handle when reporting
/// results back to `POST /api/runs/report`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentRun {
    pub id: u64,
    pub commit_sha: String,
    pub project_id: uuid::Uuid,
    pub project_upstream: String,
    pub experiment_name: String,
    /// Set when this run is part of a bisection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bisection_id: Option<u64>,
}

/// Measurement written by a forager plugin to `FORAGER_OUT`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForagerPluginOutput {
    pub name: String,
    pub value: serde_json::Value,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub tags: IndexMap<String, String>,
}

/// Envelope written by a forager plugin to `FORAGER_OUT`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForagerPluginEnvelope {
    #[serde(default)]
    pub outcomes: Vec<ForagerPluginOutput>,
}

/// Per-step report included in `ExperimentRunReport`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentRunStep {
    pub step: String,
    /// Empty when the forager produced no measurements (e.g. `exec`).
    #[serde(default)]
    pub measurements: Vec<ForagerPluginOutput>,
}

/// Body of `POST /api/runs/report` — wezel-cli reporting completed experiment
/// run results. Auth is via the caller's `wez_live_…` API token. `run_id`
/// identifies which row's results are being reported; the server resolves
/// `(commit, experiment, bisection_id)` from that row, so the report no
/// longer needs to carry them.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentRunReport {
    pub run_id: u64,
    pub steps: Vec<ExperimentRunStep>,
    /// Conclusion definitions from the experiment TOML.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub summaries: Vec<SummaryDef>,
}

/// Response from `POST /api/runs/report`.
///
/// `queue_pending` tells wezel-cli whether the server still has unclaimed
/// experiment runs for this org/project — a hint that wezel-cli can
/// re-dispatch its own workflow rather than waiting for the next scheduled
/// poll. The actual self-dispatch logic lives in wezel-cli; the field stays
/// `false` in this spec and is wired up in a follow-up.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentRunResponse {
    pub status: String,
    pub queue_pending: bool,
}

// ── Experiment PR ────────────────────────────────────────────────────────────

/// Request body for `POST /api/project/{id}/experiment/pr`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentPrRequest {
    pub experiment_name: String,
    /// Map of repo-relative path → file content.
    pub files: IndexMap<String, String>,
}

/// Response from `POST /api/project/{id}/experiment/pr`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentPrResponse {
    pub pr_url: String,
}

// ── Forager schema (sidecar emitted by `forager-<name> --schema`) ────────────

/// Self-description a forager prints in response to `--schema`. The wezel CLI
/// caches the JSON to `<plugin_dir>/forager-<name>.schema.json` at install
/// time and reads it back to compose the bundled `.wezel/schema.json` used by
/// editors for `experiment.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForagerSchema {
    /// Forager identifier; must match `forager-<name>` in the binary name.
    pub name: String,
    /// One-line human description shown in CLI listings.
    pub description: String,
    /// JSON Schema for the forager-specific input fields (everything beyond
    /// the `[step.<tool>.<name>]` header in a step table). Always a JSON
    /// Schema object.
    pub inputs: serde_json::Value,
    /// Free-form Markdown documenting the outcomes this forager emits.
    /// Spliced into the `description` of the `outcome` field in the bundled
    /// experiment schema so editors surface it on hover.
    pub outcomes_doc: String,
}

// ── Pheromone schema ──────────────────────────────────────────────────────────

/// A single field in a pheromone's schema.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PheromoneField {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deprecated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deprecated_in: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<String>,
}

/// A registered pheromone tool with its schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PheromoneInfo {
    pub id: u64,
    pub name: String,
    pub github_repo: String,
    pub version: String,
    pub platforms: Vec<String>,
    pub fields: Vec<PheromoneField>,
    pub fetched_at: String,
}

// ── Pheromone ─────────────────────────────────────────────────────────────────

/// Written by a `pheromone-<tool>` process to the path in `PHEROMONE_OUT`.
/// This is how pheromone handlers communicate back to `pheromone_cli`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PheromoneOutput {
    /// The build tool (e.g. "cargo").
    pub tool: String,
    /// The normalized subcommand (e.g. "build", "test", "check").
    pub command: String,
    /// Coarse scenario-level platform (e.g. "macOS"), if set by the pheromone.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// The detected profile, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<Profile>,
    /// Packages / crates targeted by this invocation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<String>,
    /// Crates that were (re)compiled during this build.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dirty_crates: Vec<String>,
    /// Dependency graph of the workspace.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graph: Vec<CrateTopo>,
    /// Any extra tool-specific metadata.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub extra: serde_json::Value,
}

impl PheromoneOutput {
    /// Known coarse platform identifiers for scenario distinction.
    pub const PLATFORM_MACOS: &str = "macOS";
    pub const PLATFORM_LINUX: &str = "Linux";
    pub const PLATFORM_WINDOWS: &str = "Windows";
    pub const PLATFORM_FREEBSD: &str = "FreeBSD";

    /// Returns the coarse platform string for the current OS, or `None` if
    /// the OS is not recognised.
    pub fn detect_platform() -> Option<String> {
        match std::env::consts::OS {
            "macos" => Some(Self::PLATFORM_MACOS.into()),
            "linux" => Some(Self::PLATFORM_LINUX.into()),
            "windows" => Some(Self::PLATFORM_WINDOWS.into()),
            "freebsd" => Some(Self::PLATFORM_FREEBSD.into()),
            _ => None,
        }
    }
}

/// A complete build event persisted by the CLI to `~/.wezel/events/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildEvent {
    /// Stable project UUID from `.wezel/config.toml`.
    pub project_id: uuid::Uuid,
    /// Upstream project identifier (e.g. "github.com/user/repo").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    /// Short git commit SHA at the time of the build.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Local working directory where the build ran.
    pub cwd: String,
    /// OS user who ran the build.
    pub user: String,
    /// Full machine spec detected by the CLI (always present).
    pub platform: String,
    /// ISO-8601 timestamp of when the build started.
    pub timestamp: String,
    /// Wall-clock duration of the build in milliseconds.
    pub duration_ms: u64,
    /// Exit code of the build process.
    pub exit_code: i32,
    /// Output from the pheromone handler, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pheromone: Option<PheromoneOutput>,
}
