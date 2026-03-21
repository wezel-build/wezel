use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ── DB rows ──────────────────────────────────────────────────────────────────

#[derive(FromRow, Serialize)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub upstream: String,
}

#[derive(FromRow, Serialize)]
pub struct User {
    pub username: String,
}

#[derive(FromRow, Serialize)]
pub struct Observation {
    pub id: i64,
    pub name: String,
    pub profile: String,
    pub pinned: bool,
    pub platform: Option<String>,
}

#[derive(FromRow)]
pub struct Run {
    pub id: i64,
    pub scenario_id: i64,
    pub user: String,
    pub platform: String,
    pub timestamp: String,
    pub commit_short: String,
    pub build_time_ms: i64,
}

#[derive(FromRow)]
pub struct DirtyCrate {
    pub run_id: i64,
    pub crate_name: String,
}

#[derive(FromRow)]
pub struct Commit {
    pub id: i64,
    pub sha: String,
    pub short_sha: String,
    pub author: String,
    pub message: String,
    pub timestamp: String,
    pub status: String,
}

#[derive(FromRow)]
pub struct Measurement {
    pub id: i64,
    pub commit_id: i64,
    pub name: String,
    pub kind: String,
    pub status: String,
    pub value: Option<f64>,
    pub prev_value: Option<f64>,
    pub unit: Option<String>,
    pub step: Option<String>,
}

#[derive(FromRow)]
#[expect(unused)]
pub struct ForagerToken {
    pub id: i64,
    pub commit_id: i64,
    pub benchmark_name: String,
    pub token: String,
}

#[derive(FromRow, Serialize)]
pub struct MeasurementDetail {
    pub measurement_id: i64,
    pub name: String,
    pub value: f64,
    pub prev_value: f64,
}

#[derive(FromRow)]
pub struct GraphNodeRow {
    pub name: String,
    pub version: String,
    pub external: bool,
}

#[derive(FromRow)]
pub struct GraphEdgeRow {
    pub source_name: String,
    pub dep_name: String,
    pub kind: String,
}

#[derive(FromRow)]
pub struct IdRow {
    pub id: i64,
}

#[derive(FromRow)]
pub struct IdNameRow {
    pub id: i64,
    pub name: String,
}

#[derive(FromRow)]
pub struct LatestCommit {
    pub short_sha: String,
    pub status: String,
}

// ── API responses ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct RunJson {
    pub user: String,
    pub platform: String,
    pub timestamp: String,
    pub commit: String,
    #[serde(rename = "buildTimeMs")]
    pub build_time_ms: i64,
    #[serde(rename = "dirtyCrates")]
    pub dirty_crates: Vec<String>,
}

#[derive(Serialize)]
pub struct GraphNodeJson {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub version: String,
    pub deps: Vec<String>,
    #[serde(rename = "buildDeps", skip_serializing_if = "Vec::is_empty")]
    pub build_deps: Vec<String>,
    #[serde(rename = "devDeps", skip_serializing_if = "Vec::is_empty")]
    pub dev_deps: Vec<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub external: bool,
}

#[derive(Serialize)]
pub struct ObservationJson {
    pub id: i64,
    pub name: String,
    pub profile: String,
    pub pinned: bool,
    pub platform: Option<String>,
    pub runs: Vec<RunJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graph: Option<Vec<GraphNodeJson>>,
}

#[derive(Serialize)]
pub struct MeasurementDetailJson {
    pub name: String,
    pub value: f64,
    #[serde(rename = "prevValue")]
    pub prev_value: f64,
}

#[derive(Serialize)]
pub struct MeasurementJson {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "prevValue")]
    pub prev_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub detail: Vec<MeasurementDetailJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
}

#[derive(Serialize)]
pub struct CommitJson {
    pub sha: String,
    #[serde(rename = "shortSha")]
    pub short_sha: String,
    pub author: String,
    pub message: String,
    pub timestamp: String,
    pub status: String,
    pub measurements: Vec<MeasurementJson>,
}

#[derive(Serialize)]
pub struct OverviewJson {
    #[serde(rename = "observationCount")]
    pub observation_count: i64,
    #[serde(rename = "trackedCount")]
    pub tracked_count: i64,
    #[serde(rename = "latestCommitShortSha")]
    pub latest_commit_short_sha: Option<String>,
    #[serde(rename = "latestCommitStatus")]
    pub latest_commit_status: Option<String>,
}

// ── Pheromone registry ───────────────────────────────────────────────────────

#[derive(FromRow)]
pub struct PheromoneRow {
    pub id: i64,
    pub name: String,
    pub github_repo: String,
    pub version: String,
    pub schema_json: String,
    pub viz_json: Option<String>,
    pub fetched_at: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct PheromoneFieldJson {
    pub name: String,
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub deprecated: bool,
    #[serde(rename = "deprecatedIn", skip_serializing_if = "Option::is_none")]
    pub deprecated_in: Option<String>,
    #[serde(rename = "replacedBy", skip_serializing_if = "Option::is_none")]
    pub replaced_by: Option<String>,
}

#[derive(Serialize)]
pub struct PheromoneJson {
    pub id: i64,
    pub name: String,
    #[serde(rename = "githubRepo")]
    pub github_repo: String,
    pub version: String,
    pub platforms: Vec<String>,
    pub fields: Vec<PheromoneFieldJson>,
    #[serde(rename = "fetchedAt")]
    pub fetched_at: String,
    #[serde(rename = "vizJson", skip_serializing_if = "Option::is_none")]
    pub viz_json: Option<serde_json::Value>,
}

// ── GitHub proxy ─────────────────────────────────────────────────────────────

#[derive(Clone, Serialize)]
pub struct GithubCommitJson {
    pub sha: String,
    #[serde(rename = "shortSha")]
    pub short_sha: String,
    pub author: String,
    pub message: String,
    pub timestamp: String,
    #[serde(rename = "htmlUrl")]
    pub html_url: String,
}
