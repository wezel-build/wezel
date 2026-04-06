use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use sqlx::FromRow;

// ── DB rows ──────────────────────────────────────────────────────────────────

#[derive(FromRow, Serialize)]
pub struct Repo {
    pub id: i64,
    pub upstream: String,
}

#[derive(FromRow)]
pub struct RepoRow {
    pub id: i64,
    pub upstream: String,
    pub webhook_registered: bool,
    pub project_count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoJson {
    pub id: i64,
    pub upstream: String,
    pub webhook_registered: bool,
    pub project_count: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebhookSetupJson {
    pub id: i64,
    pub upstream: String,
    pub webhook_secret: String,
    pub webhook_url: String,
    /// Whether the webhook was auto-registered on GitHub.
    pub registered: bool,
}

#[derive(FromRow, Serialize)]
pub struct Project {
    pub id: i64,
    pub repo_id: i64,
    pub name: String,
    pub subdir: String,
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
    #[expect(unused)]
    pub repo_id: i64,
    pub sha: String,
    pub short_sha: String,
    #[expect(unused)]
    pub parent_sha: Option<String>,
    pub author: String,
    pub message: String,
    pub timestamp: String,
}

#[derive(FromRow)]
pub struct Measurement {
    pub id: i64,
    pub commit_id: i64,
    #[expect(unused)]
    pub project_id: i64,
    pub name: String,
    pub status: String,
    pub value: Option<f64>,
    pub unit: Option<String>,
    pub step: Option<String>,
}

#[derive(FromRow, Serialize)]
pub struct MeasurementDetail {
    pub measurement_id: i64,
    pub name: String,
    pub value: f64,
}

#[derive(FromRow)]
pub struct MeasurementTag {
    pub measurement_id: i64,
    pub key: String,
    pub value: String,
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
}

#[derive(Serialize)]
pub struct MeasurementJson {
    pub id: i64,
    pub name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub tags: HashMap<String, String>,
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
}

#[derive(Serialize)]
pub struct ForagerQueueJobStatus {
    pub id: i64,
    pub status: String,
}

#[derive(Serialize)]
pub struct ExperimentPrResponse {
    #[serde(rename = "prUrl")]
    pub pr_url: String,
}

// ── Bisections ──────────────────────────────────────────────────────────────

#[derive(FromRow)]
pub struct Bisection {
    pub id: i64,
    pub project_id: i64,
    pub experiment_name: String,
    pub measurement_name: String,
    pub branch: String,
    pub good_sha: String,
    pub bad_sha: String,
    pub good_value: f64,
    pub bad_value: f64,
    pub status: String,
    pub culprit_sha: Option<String>,
    pub identity_tags: sqlx::types::Json<HashMap<String, String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BisectionJson {
    pub id: i64,
    pub project_id: i64,
    pub experiment_name: String,
    pub measurement_name: String,
    pub branch: String,
    pub good_sha: String,
    pub bad_sha: String,
    pub good_value: f64,
    pub bad_value: f64,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub culprit_sha: Option<String>,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub identity_tags: HashMap<String, String>,
}

impl From<Bisection> for BisectionJson {
    fn from(b: Bisection) -> Self {
        Self {
            id: b.id,
            project_id: b.project_id,
            experiment_name: b.experiment_name,
            measurement_name: b.measurement_name,
            branch: b.branch,
            good_sha: b.good_sha,
            bad_sha: b.bad_sha,
            good_value: b.good_value,
            bad_value: b.bad_value,
            status: b.status,
            culprit_sha: b.culprit_sha,
            identity_tags: b.identity_tags.0,
        }
    }
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
