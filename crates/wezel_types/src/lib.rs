//! Shared types for the wezel ecosystem.
//!
//! These mirror the data model consumed by the Anthill frontend.

use std::collections::HashMap;

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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tags: HashMap<String, String>,
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
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub identity_tags: HashMap<String, String>,
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

/// A named scalar derived from measurements, used for regression detection.
///
/// Defined in `.wezel/experiments/<name>/experiment.toml` under `[[summaries]]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryDef {
    pub name: String,
    /// Measurement name to aggregate over.
    pub measurement: String,
    pub aggregation: Aggregation,
    /// Tag key=value filters applied before aggregation.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub filter: HashMap<String, String>,
    /// Whether to trigger bisection on regression (default true).
    #[serde(default = "bool_true")]
    pub bisect: bool,
}

fn bool_true() -> bool {
    true
}

impl SummaryDef {
    /// Compute this conclusion's value from a slice of plugin measurements.
    pub fn compute(&self, measurements: &[ForagerPluginOutput]) -> Option<f64> {
        let mut values: Vec<f64> = measurements
            .iter()
            .filter(|m| m.name == self.measurement)
            .filter(|m| {
                self.filter
                    .iter()
                    .all(|(k, v)| m.tags.get(k).map(|s| s.as_str()) == Some(v.as_str()))
            })
            .filter_map(|m| m.value.as_f64())
            .collect();

        if values.is_empty() {
            return None;
        }

        Some(match self.aggregation {
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
        })
    }
}

/// A forager job with claim token, returned by `POST /api/forager/jobs/next`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForagerJob {
    pub id: u64,
    pub token: String,
    pub commit_sha: String,
    pub project_id: u64,
    pub project_upstream: String,
    pub experiment_name: String,
    /// Set when this job is part of a bisection run.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bisection_id: Option<u64>,
}

/// Measurement written by a forager plugin to `FORAGER_OUT`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForagerPluginOutput {
    pub name: String,
    pub value: serde_json::Value,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub tags: HashMap<String, String>,
}

/// Envelope written by a forager plugin to `FORAGER_OUT`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForagerPluginEnvelope {
    #[serde(default)]
    pub measurements: Vec<ForagerPluginOutput>,
}

/// Per-step report included in `ForagerRunReport`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForagerStepReport {
    pub step: String,
    /// Empty when the forager produced no measurements (e.g. `exec`).
    #[serde(default)]
    pub measurements: Vec<ForagerPluginOutput>,
}

/// Body of `POST /api/forager/run`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForagerRunReport {
    pub token: String,
    pub steps: Vec<ForagerStepReport>,
    /// Conclusion definitions from the experiment TOML.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub summaries: Vec<SummaryDef>,
    /// Forwarded from the job; lets burrow progress a bisection.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bisection_id: Option<u64>,
}

// ── Experiment PR ────────────────────────────────────────────────────────────

/// Request body for `POST /api/project/{id}/experiment/pr`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentPrRequest {
    pub experiment_name: String,
    /// Map of repo-relative path → file content.
    pub files: std::collections::HashMap<String, String>,
}

/// Response from `POST /api/project/{id}/experiment/pr`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentPrResponse {
    pub pr_url: String,
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
