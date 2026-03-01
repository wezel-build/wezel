//! Shared types for the wezel ecosystem.
//!
//! These mirror the data model consumed by the Anthill frontend.

use serde::{Deserialize, Serialize};

// ── Project ──────────────────────────────────────────────────────────────────

/// A project, identified across machines by its upstream (git remote) URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: u64,
    /// Human-readable name, e.g. "wezel" (derived from the repo name).
    pub name: String,
    /// Canonical upstream URL (e.g. "github.com/wezel-dev/wezel").
    /// Stripped of protocol and `.git` suffix so that SSH and HTTPS
    /// remotes resolve to the same identity.
    pub upstream: String,
}

// ── Dependency graph ─────────────────────────────────────────────────────────

/// A crate and its direct dependencies, forming one node in the build graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateTopo {
    pub name: String,
    pub deps: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
}

// ── Build runs ───────────────────────────────────────────────────────────────

/// A single observed build invocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub user: String,
    pub timestamp: String,
    pub commit: String,
    pub build_time_ms: u64,
    pub dirty_crates: Vec<String>,
}

// ── Scenarios ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Dev,
    Release,
}

/// A build scenario: a specific project + profile combination being tracked.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scenario {
    pub id: u64,
    pub name: String,
    pub profile: Profile,
    pub pinned: bool,
    pub graph: Vec<CrateTopo>,
    pub runs: Vec<Run>,
}

// ── Forager / commit measurements ────────────────────────────────────────────

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
pub struct MeasurementDetail {
    pub name: String,
    pub value: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_value: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Measurement {
    pub id: u64,
    pub name: String,
    pub kind: String,
    pub status: MeasurementStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<Vec<MeasurementDetail>>,
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
    pub status: CommitStatus,
    pub measurements: Vec<Measurement>,
}

/// Written by a `pheromone-<tool>` process to the path in `PHEROMONE_OUT`.
/// This is how pheromone handlers communicate back to `pheromone_cli`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PheromoneOutput {
    /// The build tool (e.g. "cargo").
    pub tool: String,
    /// The normalized subcommand (e.g. "build", "test", "check").
    pub command: String,
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

/// A complete build event persisted by the CLI to `~/.wezel/events/`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuildEvent {
    /// Upstream project identifier (e.g. "github.com/user/repo").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream: Option<String>,
    /// Local working directory where the build ran.
    pub cwd: String,
    /// OS user who ran the build.
    pub user: String,
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
