//! Shared types for the pheromone ecosystem.
//!
//! These mirror the data model consumed by the Anthill frontend.

use serde::{Deserialize, Serialize};

// ── Dependency graph ─────────────────────────────────────────────────────────

/// A crate and its direct dependencies, forming one node in the build graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateTopo {
    pub name: String,
    pub deps: Vec<String>,
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
