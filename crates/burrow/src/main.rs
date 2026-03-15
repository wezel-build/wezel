mod auth;
mod db;
mod models;

use std::collections::HashMap;

use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::Response,
    routing::{get, patch, post},
};
use axum_extra::extract::CookieJar;
use clap::Parser;
use models::*;
use wezel_types;
use reqwest::Client;
use serde::Deserialize;
use serde_json::Value;
use sqlx::PgPool;
use std::sync::{Mutex, OnceLock};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[derive(Parser)]
#[command(name = "burrow", about = "Wezel API server")]
struct Cli {
    /// Port to listen on
    #[arg(short, long, default_value = "3001")]
    port: u16,
}

type ApiResult<T> = Result<T, StatusCode>;

fn ise<E: std::fmt::Debug>(_: E) -> StatusCode {
    StatusCode::INTERNAL_SERVER_ERROR
}

// ── Helpers ──────────────────────────────────────────────────────────────────

static GITHUB_COMMIT_CACHE: OnceLock<Mutex<HashMap<String, GithubCommitJson>>> = OnceLock::new();

fn github_commit_cache() -> &'static Mutex<HashMap<String, GithubCommitJson>> {
    GITHUB_COMMIT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn github_owner_repo(upstream: &str) -> Option<(String, String)> {
    let trimmed = upstream.trim().trim_end_matches('/');

    // Normalize common schemes.
    let no_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .or_else(|| trimmed.strip_prefix("ssh://"))
        .or_else(|| trimmed.strip_prefix("git://"))
        .unwrap_or(trimmed);

    // Support git@github.com:org/repo(.git) style.
    let normalized = if let Some(rest) = no_scheme.strip_prefix("git@") {
        rest.replacen(':', "/", 1)
    } else {
        no_scheme.to_string()
    };

    let host_rest = normalized.strip_prefix("github.com/")?;
    let mut parts = host_rest.split('/');

    let owner = parts.next()?.trim();
    let repo_raw = parts.next()?.trim();

    if owner.is_empty() || repo_raw.is_empty() {
        return None;
    }

    let repo = repo_raw.strip_suffix(".git").unwrap_or(repo_raw).trim();
    if repo.is_empty() {
        return None;
    }

    Some((owner.to_string(), repo.to_string()))
}

fn github_cache_key(owner: &str, repo: &str, sha: &str) -> String {
    format!("{owner}/{repo}:{sha}")
}

async fn fetch_github_commit(
    client: &Client,
    owner: &str,
    repo: &str,
    sha: &str,
) -> ApiResult<GithubCommitJson> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{sha}");
    let mut request = client.get(url).header("User-Agent", "wezel-burrow");

    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            request = request.bearer_auth(token);
        }
    }

    let response = request.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    let status = response.status();

    if !status.is_success() {
        let code = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        return Err(code);
    }

    let body: Value = response.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    let full_sha = body
        .get("sha")
        .and_then(|v| v.as_str())
        .unwrap_or(sha)
        .to_string();
    let short_sha = full_sha.chars().take(7).collect::<String>();

    let author = body
        .get("author")
        .and_then(|v| v.get("login"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            body.get("commit")
                .and_then(|v| v.get("author"))
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("unknown")
        .to_string();

    let message = body
        .get("commit")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let timestamp = body
        .get("commit")
        .and_then(|v| v.get("author"))
        .and_then(|v| v.get("date"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            body.get("commit")
                .and_then(|v| v.get("committer"))
                .and_then(|v| v.get("date"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string();

    let html_url = body
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(GithubCommitJson {
        sha: full_sha,
        short_sha,
        author,
        message,
        timestamp,
        html_url,
    })
}

async fn get_or_fetch_github_commit(
    client: &Client,
    owner: &str,
    repo: &str,
    sha: &str,
) -> ApiResult<GithubCommitJson> {
    let key = github_cache_key(owner, repo, sha);

    if let Some(cached) = github_commit_cache()
        .lock()
        .map_err(ise)?
        .get(&key)
        .cloned()
    {
        return Ok(cached);
    }

    let commit = fetch_github_commit(client, owner, repo, sha).await?;
    github_commit_cache()
        .lock()
        .map_err(ise)?
        .insert(key, commit.clone());

    Ok(commit)
}

async fn build_graph(pool: &PgPool, scenario_id: i64) -> ApiResult<Vec<GraphNodeJson>> {
    let nodes = sqlx::query_as::<_, GraphNodeRow>(
        "SELECT name, version, external FROM graph_nodes WHERE scenario_id = $1 ORDER BY name",
    )
    .bind(scenario_id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let edges = sqlx::query_as::<_, GraphEdgeRow>(
        "SELECT src.name AS source_name, tgt.name AS dep_name, ge.kind \
         FROM graph_edges ge \
         JOIN graph_nodes src ON src.id = ge.source_id \
         JOIN graph_nodes tgt ON tgt.id = ge.target_id \
         WHERE src.scenario_id = $1",
    )
    .bind(scenario_id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let mut deps_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut build_deps_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut dev_deps_map: HashMap<String, Vec<String>> = HashMap::new();
    for node in &nodes {
        deps_map.entry(node.name.clone()).or_default();
        build_deps_map.entry(node.name.clone()).or_default();
        dev_deps_map.entry(node.name.clone()).or_default();
    }
    for edge in edges {
        let map = match edge.kind.as_str() {
            "build" => &mut build_deps_map,
            "dev" => &mut dev_deps_map,
            _ => &mut deps_map,
        };
        map.entry(edge.source_name).or_default().push(edge.dep_name);
    }

    let graph: Vec<GraphNodeJson> = nodes
        .into_iter()
        .map(|n| {
            let deps = deps_map.remove(&n.name).unwrap_or_default();
            let build_deps = build_deps_map.remove(&n.name).unwrap_or_default();
            let dev_deps = dev_deps_map.remove(&n.name).unwrap_or_default();
            GraphNodeJson {
                name: n.name,
                version: n.version,
                deps,
                build_deps,
                dev_deps,
                external: n.external,
            }
        })
        .collect();

    Ok(graph)
}

async fn observation_to_json(
    pool: &PgPool,
    id: i64,
    include_graph: bool,
) -> ApiResult<Option<ObservationJson>> {
    let Some(s) = sqlx::query_as::<_, Observation>(
        "SELECT id, name, profile, pinned, platform FROM observations WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(ise)?
    else {
        return Ok(None);
    };

    let runs = sqlx::query_as::<_, Run>(
        "SELECT id, scenario_id, \"user\", platform, timestamp, commit_short, build_time_ms \
         FROM runs WHERE scenario_id = $1 ORDER BY timestamp",
    )
    .bind(s.id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let run_ids: Vec<i64> = runs.iter().map(|r| r.id).collect();

    // Fetch all dirty crates for these runs in one query
    let dirty_crates = sqlx::query_as::<_, DirtyCrate>(
        "SELECT run_id, crate_name FROM run_dirty_crates WHERE run_id = ANY($1) ORDER BY crate_name",
    )
    .bind(&run_ids)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let mut dirty_map: HashMap<i64, Vec<String>> = HashMap::new();
    for dc in dirty_crates {
        dirty_map.entry(dc.run_id).or_default().push(dc.crate_name);
    }

    let runs_json: Vec<RunJson> = runs
        .into_iter()
        .map(|r| {
            let crates = dirty_map.remove(&r.id).unwrap_or_default();
            RunJson {
                user: r.user,
                platform: r.platform,
                timestamp: r.timestamp,
                commit: r.commit_short,
                build_time_ms: r.build_time_ms,
                dirty_crates: crates,
            }
        })
        .collect();

    let graph = if include_graph {
        let g = build_graph(pool, s.id).await?;
        if g.is_empty() { None } else { Some(g) }
    } else {
        None
    };

    Ok(Some(ObservationJson {
        id: s.id,
        name: s.name,
        profile: s.profile,
        pinned: s.pinned,
        platform: s.platform,
        runs: runs_json,
        graph,
    }))
}

async fn commit_to_json(pool: &PgPool, commit_id: i64) -> ApiResult<CommitJson> {
    let c = sqlx::query_as::<_, Commit>(
        "SELECT id, sha, short_sha, author, message, timestamp, status FROM commits WHERE id = $1",
    )
    .bind(commit_id)
    .fetch_one(pool)
    .await
    .map_err(ise)?;

    let measurements = sqlx::query_as::<_, Measurement>(
        "SELECT id, commit_id, name, kind, status, value, prev_value, unit, step \
         FROM measurements WHERE commit_id = $1 ORDER BY id",
    )
    .bind(commit_id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let m_ids: Vec<i64> = measurements.iter().map(|m| m.id).collect();

    // Fetch all details for these measurements in one query
    let details = sqlx::query_as::<_, MeasurementDetail>(
        "SELECT measurement_id, name, value, prev_value \
         FROM measurement_details WHERE measurement_id = ANY($1) ORDER BY id",
    )
    .bind(&m_ids)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let mut detail_map: HashMap<i64, Vec<MeasurementDetailJson>> = HashMap::new();
    for d in details {
        detail_map
            .entry(d.measurement_id)
            .or_default()
            .push(MeasurementDetailJson {
                name: d.name,
                value: d.value,
                prev_value: d.prev_value,
            });
    }

    let measurements_json: Vec<MeasurementJson> = measurements
        .into_iter()
        .map(|m| {
            let detail = detail_map.remove(&m.id).unwrap_or_default();
            MeasurementJson {
                id: m.id,
                name: m.name,
                kind: m.kind,
                status: m.status,
                value: m.value,
                prev_value: m.prev_value,
                unit: m.unit,
                detail,
                step: m.step,
            }
        })
        .collect();

    Ok(CommitJson {
        sha: c.sha,
        short_sha: c.short_sha,
        author: c.author,
        message: c.message,
        timestamp: c.timestamp,
        status: c.status,
        measurements: measurements_json,
    })
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn create_project(
    State(pool): State<PgPool>,
    Json(body): Json<Value>,
) -> ApiResult<(StatusCode, Json<Project>)> {
    let name = body["name"].as_str().ok_or(StatusCode::BAD_REQUEST)?;
    let upstream = body["upstream"].as_str().ok_or(StatusCode::BAD_REQUEST)?;

    let project = sqlx::query_as::<_, Project>(
        "INSERT INTO projects (name, upstream) VALUES ($1, $2) RETURNING id, name, upstream",
    )
    .bind(name)
    .bind(upstream)
    .fetch_one(&pool)
    .await
    .map_err(ise)?;

    Ok((StatusCode::CREATED, Json(project)))
}

async fn get_projects(State(pool): State<PgPool>) -> ApiResult<Json<Vec<Project>>> {
    let projects =
        sqlx::query_as::<_, Project>("SELECT id, name, upstream FROM projects ORDER BY id")
            .fetch_all(&pool)
            .await
            .map_err(ise)?;
    Ok(Json(projects))
}

async fn rename_project(
    State(pool): State<PgPool>,
    Path(project_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Project>> {
    let name = body["name"].as_str().ok_or(StatusCode::BAD_REQUEST)?;
    let project = sqlx::query_as::<_, Project>(
        "UPDATE projects SET name = $1 WHERE id = $2 RETURNING id, name, upstream",
    )
    .bind(name)
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(project))
}

async fn get_overview(State(pool): State<PgPool>) -> ApiResult<Json<OverviewJson>> {
    let (observation_count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM observations")
        .fetch_one(&pool)
        .await
        .map_err(ise)?;

    let (tracked_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM observations WHERE pinned = TRUE")
            .fetch_one(&pool)
            .await
            .map_err(ise)?;

    let latest = sqlx::query_as::<_, LatestCommit>(
        "SELECT short_sha, status FROM commits ORDER BY timestamp DESC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(ise)?;

    Ok(Json(OverviewJson {
        observation_count,
        tracked_count,
        latest_commit_short_sha: latest.as_ref().map(|l| l.short_sha.clone()),
        latest_commit_status: latest.map(|l| l.status),
    }))
}

async fn get_overview_p(
    Path((project_id,)): Path<(i64,)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<OverviewJson>> {
    let (observation_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM observations WHERE project_id = $1")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .map_err(ise)?;

    let (tracked_count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM observations WHERE project_id = $1 AND pinned = TRUE")
            .bind(project_id)
            .fetch_one(&pool)
            .await
            .map_err(ise)?;

    let latest = sqlx::query_as::<_, LatestCommit>(
        "SELECT short_sha, status FROM commits WHERE project_id = $1 ORDER BY timestamp DESC LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?;

    Ok(Json(OverviewJson {
        observation_count,
        tracked_count,
        latest_commit_short_sha: latest.as_ref().map(|l| l.short_sha.clone()),
        latest_commit_status: latest.map(|l| l.status),
    }))
}

async fn get_observations_p(
    Path((project_id,)): Path<(i64,)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<Vec<ObservationJson>>> {
    let scenarios = sqlx::query_as::<_, Observation>(
        "SELECT id, name, profile, pinned, platform FROM observations WHERE project_id = $1 ORDER BY id",
    )
    .bind(project_id)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    if scenarios.is_empty() {
        return Ok(Json(vec![]));
    }

    let scenario_ids: Vec<i64> = scenarios.iter().map(|s| s.id).collect();

    let runs = sqlx::query_as::<_, Run>(
        "SELECT id, scenario_id, \"user\", platform, timestamp, commit_short, build_time_ms \
         FROM runs WHERE scenario_id = ANY($1) ORDER BY timestamp",
    )
    .bind(&scenario_ids)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let run_ids: Vec<i64> = runs.iter().map(|r| r.id).collect();

    let dirty_crates = sqlx::query_as::<_, DirtyCrate>(
        "SELECT run_id, crate_name FROM run_dirty_crates \
         WHERE run_id = ANY($1) ORDER BY crate_name",
    )
    .bind(&run_ids)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let mut dirty_map: HashMap<i64, Vec<String>> = HashMap::new();
    for dc in dirty_crates {
        dirty_map.entry(dc.run_id).or_default().push(dc.crate_name);
    }

    let mut runs_by_scenario: HashMap<i64, Vec<RunJson>> = HashMap::new();
    for r in runs {
        let crates = dirty_map.remove(&r.id).unwrap_or_default();
        runs_by_scenario
            .entry(r.scenario_id)
            .or_default()
            .push(RunJson {
                user: r.user,
                platform: r.platform,
                timestamp: r.timestamp,
                commit: r.commit_short,
                build_time_ms: r.build_time_ms,
                dirty_crates: crates,
            });
    }

    let out: Vec<ObservationJson> = scenarios
        .into_iter()
        .map(|s| ObservationJson {
            id: s.id,
            name: s.name,
            profile: s.profile,
            pinned: s.pinned,
            platform: s.platform,
            runs: runs_by_scenario.remove(&s.id).unwrap_or_default(),
            graph: None,
        })
        .collect();

    Ok(Json(out))
}

async fn get_observations(State(pool): State<PgPool>) -> ApiResult<Json<Vec<ObservationJson>>> {
    let scenarios = sqlx::query_as::<_, Observation>(
        "SELECT id, name, profile, pinned, platform FROM observations ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    if scenarios.is_empty() {
        return Ok(Json(vec![]));
    }

    let scenario_ids: Vec<i64> = scenarios.iter().map(|s| s.id).collect();

    let runs = sqlx::query_as::<_, Run>(
        "SELECT id, scenario_id, \"user\", platform, timestamp, commit_short, build_time_ms \
         FROM runs WHERE scenario_id = ANY($1) ORDER BY timestamp",
    )
    .bind(&scenario_ids)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let run_ids: Vec<i64> = runs.iter().map(|r| r.id).collect();

    let dirty_crates = sqlx::query_as::<_, DirtyCrate>(
        "SELECT run_id, crate_name FROM run_dirty_crates \
         WHERE run_id = ANY($1) ORDER BY crate_name",
    )
    .bind(&run_ids)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let mut dirty_map: HashMap<i64, Vec<String>> = HashMap::new();
    for dc in dirty_crates {
        dirty_map.entry(dc.run_id).or_default().push(dc.crate_name);
    }

    let mut runs_by_scenario: HashMap<i64, Vec<RunJson>> = HashMap::new();
    for r in runs {
        let crates = dirty_map.remove(&r.id).unwrap_or_default();
        runs_by_scenario
            .entry(r.scenario_id)
            .or_default()
            .push(RunJson {
                user: r.user,
                platform: r.platform,
                timestamp: r.timestamp,
                commit: r.commit_short,
                build_time_ms: r.build_time_ms,
                dirty_crates: crates,
            });
    }

    let out: Vec<ObservationJson> = scenarios
        .into_iter()
        .map(|s| ObservationJson {
            id: s.id,
            name: s.name,
            profile: s.profile,
            pinned: s.pinned,
            platform: s.platform,
            runs: runs_by_scenario.remove(&s.id).unwrap_or_default(),
            graph: None,
        })
        .collect();

    Ok(Json(out))
}

async fn get_observation(
    Path(id): Path<i64>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<ObservationJson>> {
    observation_to_json(&pool, id, true)
        .await?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_observation_p(
    Path((_pid, id)): Path<(i64, i64)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<ObservationJson>> {
    observation_to_json(&pool, id, true)
        .await?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn get_commits(State(pool): State<PgPool>) -> ApiResult<Json<Vec<CommitJson>>> {
    let commits = sqlx::query_as::<_, Commit>(
        "SELECT id, sha, short_sha, author, message, timestamp, status \
         FROM commits ORDER BY timestamp",
    )
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    if commits.is_empty() {
        return Ok(Json(vec![]));
    }

    let commit_ids: Vec<i64> = commits.iter().map(|c| c.id).collect();

    let measurements = sqlx::query_as::<_, Measurement>(
        "SELECT id, commit_id, name, kind, status, value, prev_value, unit, step \
         FROM measurements WHERE commit_id = ANY($1) ORDER BY id",
    )
    .bind(&commit_ids)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let m_ids: Vec<i64> = measurements.iter().map(|m| m.id).collect();

    let details = sqlx::query_as::<_, MeasurementDetail>(
        "SELECT measurement_id, name, value, prev_value \
         FROM measurement_details WHERE measurement_id = ANY($1) ORDER BY id",
    )
    .bind(&m_ids)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let mut detail_map: HashMap<i64, Vec<MeasurementDetailJson>> = HashMap::new();
    for d in details {
        detail_map
            .entry(d.measurement_id)
            .or_default()
            .push(MeasurementDetailJson {
                name: d.name,
                value: d.value,
                prev_value: d.prev_value,
            });
    }

    let mut measurements_by_commit: HashMap<i64, Vec<MeasurementJson>> = HashMap::new();
    for m in measurements {
        measurements_by_commit
            .entry(m.commit_id)
            .or_default()
            .push(MeasurementJson {
                id: m.id,
                name: m.name,
                kind: m.kind,
                status: m.status,
                value: m.value,
                prev_value: m.prev_value,
                unit: m.unit,
                detail: detail_map.remove(&m.id).unwrap_or_default(),
                step: m.step,
            });
    }

    let out: Vec<CommitJson> = commits
        .into_iter()
        .map(|c| CommitJson {
            sha: c.sha,
            short_sha: c.short_sha,
            author: c.author,
            message: c.message,
            timestamp: c.timestamp,
            status: c.status,
            measurements: measurements_by_commit.remove(&c.id).unwrap_or_default(),
        })
        .collect();

    Ok(Json(out))
}

async fn get_commits_p(
    Path((project_id,)): Path<(i64,)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<Vec<CommitJson>>> {
    let commits = sqlx::query_as::<_, Commit>(
        "SELECT id, sha, short_sha, author, message, timestamp, status \
         FROM commits WHERE project_id = $1 ORDER BY timestamp",
    )
    .bind(project_id)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    if commits.is_empty() {
        return Ok(Json(vec![]));
    }

    let commit_ids: Vec<i64> = commits.iter().map(|c| c.id).collect();

    let measurements = sqlx::query_as::<_, Measurement>(
        "SELECT id, commit_id, name, kind, status, value, prev_value, unit, step \
         FROM measurements WHERE commit_id = ANY($1) ORDER BY id",
    )
    .bind(&commit_ids)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let m_ids: Vec<i64> = measurements.iter().map(|m| m.id).collect();

    let details = sqlx::query_as::<_, MeasurementDetail>(
        "SELECT measurement_id, name, value, prev_value \
         FROM measurement_details WHERE measurement_id = ANY($1) ORDER BY id",
    )
    .bind(&m_ids)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let mut detail_map: HashMap<i64, Vec<MeasurementDetailJson>> = HashMap::new();
    for d in details {
        detail_map
            .entry(d.measurement_id)
            .or_default()
            .push(MeasurementDetailJson {
                name: d.name,
                value: d.value,
                prev_value: d.prev_value,
            });
    }

    let mut measurements_by_commit: HashMap<i64, Vec<MeasurementJson>> = HashMap::new();
    for m in measurements {
        measurements_by_commit
            .entry(m.commit_id)
            .or_default()
            .push(MeasurementJson {
                id: m.id,
                name: m.name,
                kind: m.kind,
                status: m.status,
                value: m.value,
                prev_value: m.prev_value,
                unit: m.unit,
                detail: detail_map.remove(&m.id).unwrap_or_default(),
                step: m.step,
            });
    }

    let out: Vec<CommitJson> = commits
        .into_iter()
        .map(|c| CommitJson {
            sha: c.sha,
            short_sha: c.short_sha,
            author: c.author,
            message: c.message,
            timestamp: c.timestamp,
            status: c.status,
            measurements: measurements_by_commit.remove(&c.id).unwrap_or_default(),
        })
        .collect();

    Ok(Json(out))
}

#[derive(Deserialize)]
struct ScheduleCommitBody {
    sha: Option<String>,
}

async fn get_commit(
    Path(sha): Path<String>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<CommitJson>> {
    get_commit_inner(&sha, &pool).await
}

async fn get_commit_p(
    Path((project_id, sha)): Path<(i64, String)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<CommitJson>> {
    get_commit_inner_p(project_id, &sha, &pool).await
}

async fn get_commit_inner(sha: &str, pool: &PgPool) -> ApiResult<Json<CommitJson>> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM commits WHERE sha = $1 OR short_sha = $2")
            .bind(sha)
            .bind(sha)
            .fetch_optional(pool)
            .await
            .map_err(ise)?;

    match row {
        Some((id,)) => Ok(Json(commit_to_json(pool, id).await?)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn get_commit_inner_p(
    project_id: i64,
    sha: &str,
    pool: &PgPool,
) -> ApiResult<Json<CommitJson>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM commits WHERE project_id = $1 AND (sha = $2 OR short_sha = $3)",
    )
    .bind(project_id)
    .bind(sha)
    .bind(sha)
    .fetch_optional(pool)
    .await
    .map_err(ise)?;

    match row {
        Some((id,)) => Ok(Json(commit_to_json(pool, id).await?)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn get_github_commit_p(
    Path((project_id, sha)): Path<(i64, String)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<GithubCommitJson>> {
    let project =
        sqlx::query_as::<_, Project>("SELECT id, name, upstream FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_optional(&pool)
            .await
            .map_err(ise)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let (owner, repo) = github_owner_repo(&project.upstream).ok_or(StatusCode::BAD_REQUEST)?;
    let client = Client::new();
    let commit = get_or_fetch_github_commit(&client, &owner, &repo, &sha).await?;
    Ok(Json(commit))
}

async fn schedule_commit_p(
    Path((project_id,)): Path<(i64,)>,
    State(pool): State<PgPool>,
    Json(body): Json<ScheduleCommitBody>,
) -> ApiResult<(StatusCode, Json<CommitJson>)> {
    let Some(sha_raw) = body.sha else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let sha = sha_raw.trim();
    if sha.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let project =
        sqlx::query_as::<_, Project>("SELECT id, name, upstream FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_optional(&pool)
            .await
            .map_err(ise)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM commits WHERE project_id = $1 AND (sha = $2 OR short_sha = $3) LIMIT 1",
    )
    .bind(project_id)
    .bind(sha)
    .bind(sha)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?;

    if let Some((id,)) = existing {
        return Ok((StatusCode::OK, Json(commit_to_json(&pool, id).await?)));
    }

    let (owner, repo) = github_owner_repo(&project.upstream).ok_or(StatusCode::BAD_REQUEST)?;
    let client = Client::new();
    let gh = get_or_fetch_github_commit(&client, &owner, &repo, sha).await?;

    let commit_row: (i64,) = sqlx::query_as(
        "INSERT INTO commits (project_id, sha, short_sha, author, message, timestamp, status) \
         VALUES ($1, $2, $3, $4, $5, $6, 'not-started') RETURNING id",
    )
    .bind(project_id)
    .bind(&gh.sha)
    .bind(&gh.short_sha)
    .bind(&gh.author)
    .bind(&gh.message)
    .bind(&gh.timestamp)
    .fetch_one(&pool)
    .await
    .map_err(ise)?;

    Ok((
        StatusCode::CREATED,
        Json(commit_to_json(&pool, commit_row.0).await?),
    ))
}

async fn get_users(State(pool): State<PgPool>) -> ApiResult<Json<Vec<String>>> {
    let rows: Vec<User> = sqlx::query_as("SELECT username FROM users ORDER BY username")
        .fetch_all(&pool)
        .await
        .map_err(ise)?;
    Ok(Json(rows.into_iter().map(|u| u.username).collect()))
}

async fn toggle_observation_pin(
    Path(id): Path<i64>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<ObservationJson>> {
    toggle_observation_pin_inner(id, &pool).await
}

async fn toggle_observation_pin_p(
    Path((_pid, id)): Path<(i64, i64)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<ObservationJson>> {
    toggle_observation_pin_inner(id, &pool).await
}

async fn toggle_observation_pin_inner(id: i64, pool: &PgPool) -> ApiResult<Json<ObservationJson>> {
    let result = sqlx::query("UPDATE observations SET pinned = NOT pinned WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(ise)?;

    if result.rows_affected() == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    observation_to_json(pool, id, true)
        .await?
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

async fn ingest_events(
    State(pool): State<PgPool>,
    Json(events): Json<Vec<Value>>,
) -> ApiResult<StatusCode> {
    for event in &events {
        let Some(upstream) = event.get("upstream").and_then(|v| v.as_str()) else {
            continue;
        };
        let user = event
            .get("user")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let timestamp = event
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let duration_ms = event
            .get("durationMs")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let run_platform = event.get("platform").and_then(|v| v.as_str()).unwrap_or("");

        // Find or create project
        let name = upstream.rsplit('/').next().unwrap_or(upstream);
        let project_id: i64 =
            match sqlx::query_as::<_, (i64,)>("SELECT id FROM projects WHERE upstream = $1")
                .bind(upstream)
                .fetch_optional(&pool)
                .await
                .map_err(ise)?
            {
                Some((id,)) => id,
                None => {
                    sqlx::query_as::<_, IdRow>(
                        "INSERT INTO projects (name, upstream) VALUES ($1, $2) RETURNING id",
                    )
                    .bind(name)
                    .bind(upstream)
                    .fetch_one(&pool)
                    .await
                    .map_err(ise)?
                    .id
                }
            };

        // Ensure user exists
        sqlx::query("INSERT INTO users (username) VALUES ($1) ON CONFLICT (username) DO NOTHING")
            .bind(user)
            .execute(&pool)
            .await
            .map_err(ise)?;

        // Process pheromone data if present
        let Some(pheromone) = event.get("pheromone") else {
            continue;
        };
        let tool = pheromone
            .get("tool")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let command = pheromone
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("build");
        let profile = pheromone
            .get("profile")
            .and_then(|v| v.as_str())
            .unwrap_or("dev");
        let packages: Vec<&str> = pheromone
            .get("packages")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        let scenario_platform: Option<&str> = pheromone.get("platform").and_then(|v| v.as_str());

        let benchmark_name = if packages.is_empty() {
            format!("{tool} {command}")
        } else {
            format!("{tool} {command} {}", packages.join(" "))
        };

        // Find or create scenario
        let scenario_id: i64 = match if let Some(sp) = scenario_platform {
            sqlx::query_as::<_, (i64,)>(
                "SELECT id FROM observations \
                 WHERE project_id = $1 AND name = $2 AND profile = $3 AND platform = $4",
            )
            .bind(project_id)
            .bind(&benchmark_name)
            .bind(profile)
            .bind(sp)
            .fetch_optional(&pool)
            .await
        } else {
            sqlx::query_as::<_, (i64,)>(
                "SELECT id FROM observations \
                 WHERE project_id = $1 AND name = $2 AND profile = $3 AND platform IS NULL",
            )
            .bind(project_id)
            .bind(&benchmark_name)
            .bind(profile)
            .fetch_optional(&pool)
            .await
        }
        .map_err(ise)?
        {
            Some((id,)) => id,
            None => {
                sqlx::query_as::<_, IdRow>(
                    "INSERT INTO observations (project_id, name, profile, platform) \
                     VALUES ($1, $2, $3, $4) RETURNING id",
                )
                .bind(project_id)
                .bind(&benchmark_name)
                .bind(profile)
                .bind(scenario_platform)
                .fetch_one(&pool)
                .await
                .map_err(ise)?
                .id
            }
        };

        // Upsert dependency graph
        if let Some(graph) = pheromone.get("graph").and_then(|v| v.as_array()) {
            // Clear old graph (CASCADE handles edges)
            sqlx::query("DELETE FROM graph_nodes WHERE scenario_id = $1")
                .bind(scenario_id)
                .execute(&pool)
                .await
                .map_err(ise)?;

            // Bulk-insert all nodes in one query, carrying version and external flag.
            let mut node_names: Vec<&str> = Vec::new();
            let mut node_versions: Vec<&str> = Vec::new();
            let mut node_externals: Vec<bool> = Vec::new();
            for e in graph {
                if let Some(name) = e.get("name").and_then(|v| v.as_str()) {
                    node_names.push(name);
                    node_versions.push(e.get("version").and_then(|v| v.as_str()).unwrap_or(""));
                    node_externals
                        .push(e.get("external").and_then(|v| v.as_bool()).unwrap_or(false));
                }
            }

            let inserted_nodes = sqlx::query_as::<_, IdNameRow>(
                "INSERT INTO graph_nodes (scenario_id, name, version, external) \
                 SELECT $1, unnest($2::text[]), unnest($3::text[]), unnest($4::bool[]) \
                 RETURNING id, name",
            )
            .bind(scenario_id)
            .bind(&node_names)
            .bind(&node_versions)
            .bind(&node_externals)
            .fetch_all(&pool)
            .await
            .map_err(ise)?;

            let node_ids: HashMap<&str, i64> = inserted_nodes
                .iter()
                .map(|r| (r.name.as_str(), r.id))
                .collect();

            // Collect all edges with their kind, then bulk-insert.
            let mut source_ids: Vec<i64> = Vec::new();
            let mut target_ids: Vec<i64> = Vec::new();
            let mut edge_kinds: Vec<&str> = Vec::new();

            let push_edges = |deps: Option<&serde_json::Value>,
                              kind: &'static str,
                              src_id: i64,
                              node_ids: &HashMap<&str, i64>,
                              source_ids: &mut Vec<i64>,
                              target_ids: &mut Vec<i64>,
                              edge_kinds: &mut Vec<&str>| {
                if let Some(arr) = deps.and_then(|v| v.as_array()) {
                    for dep in arr {
                        if let Some(dep_name) = dep.as_str()
                            && let Some(&tgt_id) = node_ids.get(dep_name)
                        {
                            source_ids.push(src_id);
                            target_ids.push(tgt_id);
                            edge_kinds.push(kind);
                        }
                    }
                }
            };

            for entry in graph {
                let Some(source_name) = entry.get("name").and_then(|v| v.as_str()) else {
                    continue;
                };
                let Some(&src_id) = node_ids.get(source_name) else {
                    continue;
                };
                push_edges(
                    entry.get("deps"),
                    "normal",
                    src_id,
                    &node_ids,
                    &mut source_ids,
                    &mut target_ids,
                    &mut edge_kinds,
                );
                push_edges(
                    entry.get("buildDeps"),
                    "build",
                    src_id,
                    &node_ids,
                    &mut source_ids,
                    &mut target_ids,
                    &mut edge_kinds,
                );
                push_edges(
                    entry.get("devDeps"),
                    "dev",
                    src_id,
                    &node_ids,
                    &mut source_ids,
                    &mut target_ids,
                    &mut edge_kinds,
                );
            }

            if !source_ids.is_empty() {
                sqlx::query(
                    "INSERT INTO graph_edges (source_id, target_id, kind) \
                     SELECT unnest($1::bigint[]), unnest($2::bigint[]), unnest($3::text[]) \
                     ON CONFLICT DO NOTHING",
                )
                .bind(&source_ids)
                .bind(&target_ids)
                .bind(&edge_kinds)
                .execute(&pool)
                .await
                .map_err(ise)?;
            }
        }

        // Insert run
        let commit_short = event.get("commit").and_then(|v| v.as_str()).unwrap_or("");

        let run_row = sqlx::query_as::<_, IdRow>(
            "INSERT INTO runs (scenario_id, \"user\", platform, timestamp, commit_short, build_time_ms) \
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(scenario_id)
        .bind(user)
        .bind(run_platform)
        .bind(timestamp)
        .bind(commit_short)
        .bind(duration_ms as i64)
        .fetch_one(&pool)
        .await
        .map_err(ise)?;

        // Bulk-insert dirty crates
        let dirty_crates: Vec<&str> = pheromone
            .get("dirtyCrates")
            .and_then(|v| v.as_array())
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        if !dirty_crates.is_empty() {
            sqlx::query(
                "INSERT INTO run_dirty_crates (run_id, crate_name) \
                 SELECT $1, unnest($2::text[]) ON CONFLICT DO NOTHING",
            )
            .bind(run_row.id)
            .bind(&dirty_crates)
            .execute(&pool)
            .await
            .map_err(ise)?;
        }
    }

    Ok(StatusCode::OK)
}

// ── Forager endpoints ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ForagerClaimBody {
    project_upstream: String,
    commit_sha: String,
    benchmark_name: String,
    // Optional commit metadata for when GitHub API is not available.
    commit_author: Option<String>,
    commit_message: Option<String>,
    commit_timestamp: Option<String>,
}

async fn post_forager_claim(
    State(pool): State<PgPool>,
    Json(body): Json<ForagerClaimBody>,
) -> ApiResult<Json<wezel_types::ForagerJob>> {
    let upstream = body.project_upstream.trim();
    let sha = body.commit_sha.trim();
    let benchmark_name = body.benchmark_name.trim();

    if upstream.is_empty() || sha.is_empty() || benchmark_name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Find or create project.
    let project_name = upstream.rsplit('/').next().unwrap_or(upstream);
    let project_id: i64 =
        match sqlx::query_as::<_, (i64,)>("SELECT id FROM projects WHERE upstream = $1")
            .bind(upstream)
            .fetch_optional(&pool)
            .await
            .map_err(ise)?
        {
            Some((id,)) => id,
            None => {
                sqlx::query_as::<_, IdRow>(
                    "INSERT INTO projects (name, upstream) VALUES ($1, $2) RETURNING id",
                )
                .bind(project_name)
                .bind(upstream)
                .fetch_one(&pool)
                .await
                .map_err(ise)?
                .id
            }
        };

    // Find or create commit.
    let short_sha: String = sha.chars().take(7).collect();
    let commit_id: i64 =
        match sqlx::query_as::<_, (i64,)>(
            "SELECT id FROM commits WHERE project_id = $1 AND (sha = $2 OR short_sha = $3)",
        )
        .bind(project_id)
        .bind(sha)
        .bind(&short_sha)
        .fetch_optional(&pool)
        .await
        .map_err(ise)?
        {
            Some((id,)) => id,
            None => {
                let author = body
                    .commit_author
                    .as_deref()
                    .unwrap_or("unknown")
                    .to_string();
                let message = body
                    .commit_message
                    .as_deref()
                    .unwrap_or("")
                    .to_string();
                let timestamp = body
                    .commit_timestamp
                    .as_deref()
                    .unwrap_or("")
                    .to_string();
                sqlx::query_as::<_, IdRow>(
                    "INSERT INTO commits \
                     (project_id, sha, short_sha, author, message, timestamp, status) \
                     VALUES ($1, $2, $3, $4, $5, $6, 'not-started') RETURNING id",
                )
                .bind(project_id)
                .bind(sha)
                .bind(&short_sha)
                .bind(&author)
                .bind(&message)
                .bind(&timestamp)
                .fetch_one(&pool)
                .await
                .map_err(ise)?
                .id
            }
        };

    // Set commit status to running.
    sqlx::query("UPDATE commits SET status = 'running' WHERE id = $1")
        .bind(commit_id)
        .execute(&pool)
        .await
        .map_err(ise)?;

    // Create forager token (expires in 4 hours).
    let token = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO forager_tokens (commit_id, benchmark_name, token, expires_at) \
         VALUES ($1, $2, $3, now() + interval '4 hours')",
    )
    .bind(commit_id)
    .bind(benchmark_name)
    .bind(&token)
    .execute(&pool)
    .await
    .map_err(ise)?;

    Ok(Json(wezel_types::ForagerJob {
        token,
        commit_sha: sha.to_string(),
        project_id: project_id as u64,
        project_upstream: upstream.to_string(),
        benchmark_name: benchmark_name.to_string(),
    }))
}

async fn post_forager_run(
    State(pool): State<PgPool>,
    Json(body): Json<wezel_types::ForagerRunReport>,
) -> ApiResult<StatusCode> {
    // Validate token and get commit_id.
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT commit_id FROM forager_tokens \
         WHERE token = $1 AND expires_at > now()",
    )
    .bind(&body.token)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?;

    let (commit_id,) = row.ok_or(StatusCode::UNAUTHORIZED)?;

    // Get project_id for prev_value lookups.
    let (project_id,): (i64,) =
        sqlx::query_as("SELECT project_id FROM commits WHERE id = $1")
            .bind(commit_id)
            .fetch_one(&pool)
            .await
            .map_err(ise)?;

    // Insert measurements.
    for step_report in &body.steps {
        let Some(ref m) = step_report.measurement else {
            continue;
        };

        // Look up prev value from most recent complete measurement with same name.
        let prev_value: Option<f64> = sqlx::query_as::<_, (Option<f64>,)>(
            "SELECT m.value FROM measurements m \
             JOIN commits c ON m.commit_id = c.id \
             WHERE c.project_id = $1 AND m.name = $2 AND m.commit_id != $3 \
             AND m.status = 'complete' \
             ORDER BY c.timestamp DESC, m.id DESC \
             LIMIT 1",
        )
        .bind(project_id)
        .bind(&m.name)
        .bind(commit_id)
        .fetch_optional(&pool)
        .await
        .map_err(ise)?
        .and_then(|(v,)| v);

        let (measurement_id,): (i64,) = sqlx::query_as(
            "INSERT INTO measurements \
             (commit_id, name, kind, status, value, prev_value, unit, step) \
             VALUES ($1, $2, $3, 'complete', $4, $5, $6, $7) RETURNING id",
        )
        .bind(commit_id)
        .bind(&m.name)
        .bind(&m.kind)
        .bind(m.value)
        .bind(prev_value)
        .bind(&m.unit)
        .bind(&step_report.step)
        .fetch_one(&pool)
        .await
        .map_err(ise)?;

        // Insert detail rows.
        for detail in &m.detail {
            sqlx::query(
                "INSERT INTO measurement_details (measurement_id, name, value, prev_value) \
                 VALUES ($1, $2, $3, 0)",
            )
            .bind(measurement_id)
            .bind(&detail.name)
            .bind(detail.value)
            .execute(&pool)
            .await
            .map_err(ise)?;
        }
    }

    // Mark commit complete.
    sqlx::query("UPDATE commits SET status = 'complete' WHERE id = $1")
        .bind(commit_id)
        .execute(&pool)
        .await
        .map_err(ise)?;

    Ok(StatusCode::OK)
}

async fn get_health() -> Json<Value> {
    Json(serde_json::json!({"status": "ok"}))
}

// ── Pheromone registry ────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct AdminPheromoneBody {
    github_repo: String,
}

#[derive(serde::Deserialize)]
struct BenchmarkPrBody {
    #[serde(rename = "benchmarkName")]
    benchmark_name: String,
    files: std::collections::HashMap<String, String>,
}

fn pheromone_json_from_row(row: &models::PheromoneRow) -> models::PheromoneJson {
    let schema: Value = serde_json::from_str(&row.schema_json).unwrap_or(Value::Null);
    let platforms = schema
        .get("platforms")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let fields = schema
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|f| models::PheromoneFieldJson {
                    name: f
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    field_type: f
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    description: f
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    deprecated: f
                        .get("deprecated")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    deprecated_in: f
                        .get("deprecatedIn")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    replaced_by: f
                        .get("replacedBy")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default();
    let viz_json = row
        .viz_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());
    models::PheromoneJson {
        id: row.id,
        name: row.name.clone(),
        github_repo: row.github_repo.clone(),
        version: row.version.clone(),
        platforms,
        fields,
        fetched_at: row.fetched_at.clone(),
        viz_json,
    }
}

fn get_dev_dir() -> Option<std::path::PathBuf> {
    std::env::var("WEZEL_PHEROMONE_DEV_DIR").ok().map(Into::into)
}

async fn fetch_and_store_from_local(
    pool: &PgPool,
    dev_dir: &std::path::Path,
    name: &str,
    github_repo: &str,
) -> ApiResult<models::PheromoneJson> {
    let schema: Value = {
        let s = std::fs::read_to_string(dev_dir.join(name).join("schema.json")).map_err(ise)?;
        serde_json::from_str(&s).map_err(ise)?
    };
    let pheromone_name = schema
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(name)
        .to_string();
    let version = schema
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();
    let schema_str = serde_json::to_string(&schema).map_err(ise)?;

    let viz_str: Option<String> = std::fs::read_to_string(dev_dir.join(name).join("viz.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| serde_json::to_string(&v).ok());

    let row = sqlx::query_as::<_, models::PheromoneRow>(
        "INSERT INTO pheromones (name, github_repo, version, schema_json, viz_json)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (name) DO UPDATE SET github_repo = $2, version = $3, schema_json = $4, viz_json = $5, fetched_at = now()
         RETURNING id, name, github_repo, version, schema_json, viz_json, fetched_at::TEXT as fetched_at",
    )
    .bind(&pheromone_name)
    .bind(github_repo)
    .bind(&version)
    .bind(&schema_str)
    .bind(&viz_str)
    .fetch_one(pool)
    .await
    .map_err(ise)?;

    let _ = sqlx::query(
        "INSERT INTO pheromone_schema_history (pheromone_id, version, schema_json)
         VALUES ($1, $2, $3)
         ON CONFLICT (pheromone_id, version) DO NOTHING",
    )
    .bind(row.id)
    .bind(&version)
    .bind(&schema_str)
    .execute(pool)
    .await;

    tracing::info!("loaded dev pheromone: {pheromone_name}");
    Ok(pheromone_json_from_row(&row))
}

async fn load_dev_pheromones(pool: &PgPool, dev_dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dev_dir) else {
        tracing::warn!("WEZEL_PHEROMONE_DEV_DIR not readable: {}", dev_dir.display());
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("schema.json").exists() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Look up existing DB row to get stored github_repo; fall back to "dev/{name}".
        let github_repo = sqlx::query_scalar::<_, String>(
            "SELECT github_repo FROM pheromones WHERE name = $1",
        )
        .bind(name.as_ref())
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| format!("dev/{name}"));

        if let Err(e) = fetch_and_store_pheromone(pool, &github_repo).await {
            tracing::warn!("failed to load dev pheromone {name}: {e:?}");
        }
    }
}

async fn fetch_and_store_pheromone(
    pool: &PgPool,
    github_repo: &str,
) -> ApiResult<models::PheromoneJson> {
    let name = github_repo.split('/').last().unwrap_or(github_repo);
    if let Some(dev_dir) = get_dev_dir() {
        if dev_dir.join(name).join("schema.json").exists() {
            return fetch_and_store_from_local(pool, &dev_dir, name, github_repo).await;
        }
    }

    let client = Client::new();
    let url = format!("https://api.github.com/repos/{github_repo}/releases/latest");
    let mut req = client.get(&url).header("User-Agent", "wezel-burrow");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            req = req.bearer_auth(token);
        }
    }
    let resp = req.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !resp.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let release: Value = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    let assets = release
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_GATEWAY)?;
    let schema_url = assets
        .iter()
        .find(|a| a.get("name").and_then(|v| v.as_str()) == Some("schema.json"))
        .and_then(|a| a.get("browser_download_url"))
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::NOT_FOUND)?
        .to_string();

    let mut schema_req = client.get(&schema_url).header("User-Agent", "wezel-burrow");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            schema_req = schema_req.bearer_auth(token);
        }
    }
    let schema_resp = schema_req
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let schema: Value = schema_resp
        .json()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let name = schema
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| github_repo.split('/').last().unwrap_or(github_repo))
        .to_string();
    let version = schema
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();
    let schema_str = serde_json::to_string(&schema).map_err(ise)?;

    // Optionally fetch viz.json from the same release.
    let viz_str: Option<String> = if let Some(viz_url) = assets
        .iter()
        .find(|a| a.get("name").and_then(|v| v.as_str()) == Some("viz.json"))
        .and_then(|a| a.get("browser_download_url"))
        .and_then(|v| v.as_str())
    {
        let mut req = client.get(viz_url).header("User-Agent", "wezel-burrow");
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            let token = token.trim().to_string();
            if !token.is_empty() {
                req = req.bearer_auth(token);
            }
        }
        if let Ok(resp) = req.send().await {
            if let Ok(val) = resp.json::<Value>().await {
                serde_json::to_string(&val).ok()
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let row = sqlx::query_as::<_, models::PheromoneRow>(
        "INSERT INTO pheromones (name, github_repo, version, schema_json, viz_json)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (name) DO UPDATE SET github_repo = $2, version = $3, schema_json = $4, viz_json = $5, fetched_at = now()
         RETURNING id, name, github_repo, version, schema_json, viz_json, fetched_at::TEXT as fetched_at",
    )
    .bind(&name)
    .bind(github_repo)
    .bind(&version)
    .bind(&schema_str)
    .bind(&viz_str)
    .fetch_one(pool)
    .await
    .map_err(ise)?;

    let _ = sqlx::query(
        "INSERT INTO pheromone_schema_history (pheromone_id, version, schema_json)
         VALUES ($1, $2, $3)
         ON CONFLICT (pheromone_id, version) DO NOTHING",
    )
    .bind(row.id)
    .bind(&version)
    .bind(&schema_str)
    .execute(pool)
    .await;

    Ok(pheromone_json_from_row(&row))
}

async fn get_pheromones(
    State(pool): State<PgPool>,
) -> ApiResult<Json<Vec<models::PheromoneJson>>> {
    let rows = sqlx::query_as::<_, models::PheromoneRow>(
        "SELECT id, name, github_repo, version, schema_json, viz_json, fetched_at::TEXT as fetched_at
         FROM pheromones ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .map_err(ise)?;
    Ok(Json(rows.iter().map(pheromone_json_from_row).collect()))
}

async fn get_admin_pheromones(
    State(pool): State<PgPool>,
) -> ApiResult<Json<Vec<models::PheromoneJson>>> {
    get_pheromones(State(pool)).await
}

async fn post_admin_pheromone(
    State(pool): State<PgPool>,
    Json(body): Json<AdminPheromoneBody>,
) -> ApiResult<(StatusCode, Json<models::PheromoneJson>)> {
    let pheromone = fetch_and_store_pheromone(&pool, &body.github_repo).await?;
    Ok((StatusCode::CREATED, Json(pheromone)))
}

async fn post_admin_pheromone_fetch(
    Path(name): Path<String>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<models::PheromoneJson>> {
    let row = sqlx::query_as::<_, models::PheromoneRow>(
        "SELECT id, name, github_repo, version, schema_json, fetched_at::TEXT as fetched_at
         FROM pheromones WHERE name = $1",
    )
    .bind(&name)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let pheromone = fetch_and_store_pheromone(&pool, &row.github_repo).await?;
    Ok(Json(pheromone))
}

// ── GitHub PR endpoint ────────────────────────────────────────────────────────

async fn github_api<T: serde::de::DeserializeOwned>(
    client: &Client,
    method: reqwest::Method,
    url: &str,
    token: &str,
    body: Option<Value>,
) -> ApiResult<T> {
    let mut req = client
        .request(method, url)
        .header("User-Agent", "wezel-burrow")
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(token);
    if let Some(b) = body {
        req = req.json(&b);
    }
    let resp = req.send().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        return Err(StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_GATEWAY));
    }
    resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)
}

async fn post_benchmark_pr(
    Path(project_id): Path<i64>,
    State(pool): State<PgPool>,
    Json(body): Json<BenchmarkPrBody>,
) -> ApiResult<Json<Value>> {
    let project =
        sqlx::query_as::<_, Project>("SELECT id, name, upstream FROM projects WHERE id = $1")
            .bind(project_id)
            .fetch_optional(&pool)
            .await
            .map_err(ise)?
            .ok_or(StatusCode::NOT_FOUND)?;

    let (owner, repo) = github_owner_repo(&project.upstream).ok_or(StatusCode::BAD_REQUEST)?;

    let token = std::env::var("GITHUB_TOKEN")
        .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let token = token.trim();
    if token.is_empty() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let client = Client::new();

    // Get repo default branch
    let repo_info: Value = github_api(
        &client,
        reqwest::Method::GET,
        &format!("https://api.github.com/repos/{owner}/{repo}"),
        token,
        None,
    )
    .await?;
    let default_branch = repo_info["default_branch"]
        .as_str()
        .unwrap_or("main")
        .to_string();

    // Get branch SHA
    let ref_info: Value = github_api(
        &client,
        reqwest::Method::GET,
        &format!(
            "https://api.github.com/repos/{owner}/{repo}/git/ref/heads/{default_branch}"
        ),
        token,
        None,
    )
    .await?;
    let base_sha = ref_info["object"]["sha"]
        .as_str()
        .ok_or(StatusCode::BAD_GATEWAY)?
        .to_string();

    // Create blobs for each file
    let mut tree_items = Vec::new();
    for (path, content) in &body.files {
        let blob: Value = github_api(
            &client,
            reqwest::Method::POST,
            &format!("https://api.github.com/repos/{owner}/{repo}/git/blobs"),
            token,
            Some(serde_json::json!({
                "content": content,
                "encoding": "utf-8"
            })),
        )
        .await?;
        let blob_sha = blob["sha"].as_str().ok_or(StatusCode::BAD_GATEWAY)?;
        tree_items.push(serde_json::json!({
            "path": path,
            "mode": "100644",
            "type": "blob",
            "sha": blob_sha
        }));
    }

    // Create tree
    let tree: Value = github_api(
        &client,
        reqwest::Method::POST,
        &format!("https://api.github.com/repos/{owner}/{repo}/git/trees"),
        token,
        Some(serde_json::json!({
            "base_tree": base_sha,
            "tree": tree_items
        })),
    )
    .await?;
    let tree_sha = tree["sha"].as_str().ok_or(StatusCode::BAD_GATEWAY)?;

    // Create commit
    let commit: Value = github_api(
        &client,
        reqwest::Method::POST,
        &format!("https://api.github.com/repos/{owner}/{repo}/git/commits"),
        token,
        Some(serde_json::json!({
            "message": format!("wezel: add {} benchmark", body.benchmark_name),
            "tree": tree_sha,
            "parents": [base_sha]
        })),
    )
    .await?;
    let commit_sha = commit["sha"].as_str().ok_or(StatusCode::BAD_GATEWAY)?;

    // Create branch
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let branch_name = format!("wezel/benchmark-{}-{ts}", body.benchmark_name);
    let _: Value = github_api(
        &client,
        reqwest::Method::POST,
        &format!("https://api.github.com/repos/{owner}/{repo}/git/refs"),
        token,
        Some(serde_json::json!({
            "ref": format!("refs/heads/{branch_name}"),
            "sha": commit_sha
        })),
    )
    .await?;

    // Create PR
    let pr: Value = github_api(
        &client,
        reqwest::Method::POST,
        &format!("https://api.github.com/repos/{owner}/{repo}/pulls"),
        token,
        Some(serde_json::json!({
            "title": format!("wezel: add {} benchmark", body.benchmark_name),
            "head": branch_name,
            "base": default_branch,
            "body": "This PR was created by [wezel](https://wezel.dev) to add a new benchmark."
        })),
    )
    .await?;
    let pr_url = pr["html_url"].as_str().ok_or(StatusCode::BAD_GATEWAY)?;

    Ok(Json(serde_json::json!({ "prUrl": pr_url })))
}

async fn require_auth(
    State(pool): State<PgPool>,
    jar: CookieJar,
    mut req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_required =
        std::env::var("GITHUB_CLIENT_ID").is_ok() || std::env::var("GITHUB_ORG").is_ok();
    if !auth_required {
        return Ok(next.run(req).await);
    }

    let session_id = jar
        .get("session_id")
        .map(|c| c.value().to_string())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let login = db::get_session(&pool, &session_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    req.extensions_mut().insert(auth::AuthUser { login });
    Ok(next.run(req).await)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    let db_url = db::db_url();
    println!("Connecting to database at {db_url}");
    let pool = db::connect(&db_url)
        .await
        .expect("could not connect to database");

    if let Some(dev_dir) = get_dev_dir() {
        load_dev_pheromones(&pool, &dev_dir).await;
    }

    // Protected API routes (all except /api/events)
    let protected_api = Router::new()
        .route("/api/project", get(get_projects).post(create_project))
        .route("/api/project/{project_id}", patch(rename_project))
        .route("/api/project/{project_id}/overview", get(get_overview_p))
        .route("/api/project/{project_id}/observation", get(get_observations_p))
        .route(
            "/api/project/{project_id}/observation/{id}",
            get(get_observation_p),
        )
        .route(
            "/api/project/{project_id}/observation/{id}/pin",
            patch(toggle_observation_pin_p),
        )
        .route(
            "/api/project/{project_id}/commit",
            get(get_commits_p).post(schedule_commit_p),
        )
        .route("/api/project/{project_id}/commit/{sha}", get(get_commit_p))
        .route(
            "/api/project/{project_id}/github/commit/{sha}",
            get(get_github_commit_p),
        )
        .route("/api/project/{project_id}/user", get(get_users))
        .route(
            "/api/project/{project_id}/benchmark/pr",
            post(post_benchmark_pr),
        )
        .route(
            "/api/admin/pheromone",
            get(get_admin_pheromones).post(post_admin_pheromone),
        )
        .route(
            "/api/admin/pheromone/{name}/fetch",
            post(post_admin_pheromone_fetch),
        )
        .route("/api/overview", get(get_overview))
        .route("/api/observation", get(get_observations))
        .route("/api/observation/{id}", get(get_observation))
        .route("/api/observation/{id}/pin", patch(toggle_observation_pin))
        .route("/api/commit", get(get_commits))
        .route("/api/commit/{sha}", get(get_commit))
        .route("/api/user", get(get_users))
        .route_layer(middleware::from_fn_with_state(pool.clone(), require_auth));

    let app = Router::new()
        .merge(protected_api)
        // Unauthenticated: ingest, forager, and auth routes
        .route("/api/events", post(ingest_events))
        .route("/api/forager/claim", post(post_forager_claim))
        .route("/api/forager/run", post(post_forager_run))
        .route("/api/pheromones", get(get_pheromones))
        .route("/auth/github", get(auth::login))
        .route("/auth/github/callback", get(auth::callback))
        .route("/auth/me", get(auth::me))
        .route("/auth/config", get(auth::config))
        .route("/auth/logout", post(auth::logout))
        .route("/health", get(get_health))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(pool);

    let addr = format!("0.0.0.0:{}", cli.port);
    println!("Burrow listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await
        .unwrap();
}
