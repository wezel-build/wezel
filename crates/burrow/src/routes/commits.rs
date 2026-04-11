use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use sqlx::PgPool;

use crate::github::{get_or_fetch_github_commit, github_owner_repo};
use crate::models::*;
use crate::{ApiResult, AppState, ise};

// ── Helper ────────────────────────────────────────────────────────────────────

/// Load tags for a batch of measurement IDs, returning a map of measurement_id -> tags.
pub async fn load_tags(
    pool: &PgPool,
    m_ids: &[i64],
) -> Result<HashMap<i64, HashMap<String, String>>, StatusCode> {
    let tag_rows = sqlx::query_as::<_, MeasurementTag>(
        "SELECT measurement_id, key, value \
         FROM measurement_tags WHERE measurement_id = ANY($1)",
    )
    .bind(m_ids)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let mut tag_map: HashMap<i64, HashMap<String, String>> = HashMap::new();
    for t in tag_rows {
        tag_map
            .entry(t.measurement_id)
            .or_default()
            .insert(t.key, t.value);
    }
    Ok(tag_map)
}

/// Build a list of MeasurementJson from Measurement rows, batch-loading tags.
fn build_measurement_json(
    measurements: Vec<Measurement>,
    tag_map: &mut HashMap<i64, HashMap<String, String>>,
) -> Vec<MeasurementJson> {
    measurements
        .into_iter()
        .map(|m| MeasurementJson {
            id: m.id,
            name: m.name,
            status: m.status,
            value: m.value.map(|v| v.0),
            tags: tag_map.remove(&m.id).unwrap_or_default(),
            step: m.step,
            experiment_name: m.experiment_name,
        })
        .collect()
}

/// Load summary values for a batch of commit IDs.
pub async fn load_summaries(
    pool: &PgPool,
    commit_ids: &[i64],
) -> Result<HashMap<i64, Vec<crate::models::SummaryValueJson>>, StatusCode> {
    let rows: Vec<(i64, String, String, f64)> = sqlx::query_as(
        "SELECT commit_id, experiment_name, name, value \
         FROM summary_values WHERE commit_id = ANY($1)",
    )
    .bind(commit_ids)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let mut map: HashMap<i64, Vec<crate::models::SummaryValueJson>> = HashMap::new();
    for (commit_id, experiment_name, name, value) in rows {
        map.entry(commit_id)
            .or_default()
            .push(crate::models::SummaryValueJson {
                experiment_name,
                name,
                value,
            });
    }
    Ok(map)
}

async fn commit_to_json(pool: &PgPool, commit_id: i64) -> ApiResult<CommitJson> {
    let c = sqlx::query_as::<_, Commit>(
        "SELECT id, repo_id, sha, short_sha, parent_sha, author, message, timestamp \
         FROM commits WHERE id = $1",
    )
    .bind(commit_id)
    .fetch_one(pool)
    .await
    .map_err(ise)?;

    let measurements = sqlx::query_as::<_, Measurement>(
        "SELECT id, commit_id, project_id, name, status, value, step, experiment_name \
         FROM measurements WHERE commit_id = $1 ORDER BY id",
    )
    .bind(commit_id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let m_ids: Vec<i64> = measurements.iter().map(|m| m.id).collect();
    let mut tag_map = load_tags(pool, &m_ids).await?;
    let measurements_json = build_measurement_json(measurements, &mut tag_map);

    let summaries = load_summaries(pool, &[c.id])
        .await?
        .remove(&c.id)
        .unwrap_or_default();

    Ok(CommitJson {
        sha: c.sha,
        short_sha: c.short_sha,
        author: c.author,
        message: c.message,
        timestamp: c.timestamp,
        measurements: measurements_json,
        summaries,
    })
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn get_commits(State(pool): State<PgPool>) -> ApiResult<Json<Vec<CommitJson>>> {
    let commits = sqlx::query_as::<_, Commit>(
        "SELECT id, repo_id, sha, short_sha, parent_sha, author, message, timestamp \
         FROM commits ORDER BY timestamp",
    )
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    build_commit_list(commits, &pool).await
}

pub async fn get_project_commits(
    Path((project_id,)): Path<(i64,)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<Vec<CommitJson>>> {
    let commits = sqlx::query_as::<_, Commit>(
        "SELECT DISTINCT c.id, c.repo_id, c.sha, c.short_sha, c.parent_sha, \
                c.author, c.message, c.timestamp \
         FROM commits c \
         JOIN measurements m ON m.commit_id = c.id AND m.project_id = $1 \
         ORDER BY c.timestamp",
    )
    .bind(project_id)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    build_commit_list(commits, &pool).await
}

async fn build_commit_list(
    commits: Vec<Commit>,
    pool: &PgPool,
) -> ApiResult<Json<Vec<CommitJson>>> {
    if commits.is_empty() {
        return Ok(Json(vec![]));
    }

    let commit_ids: Vec<i64> = commits.iter().map(|c| c.id).collect();

    let measurements = sqlx::query_as::<_, Measurement>(
        "SELECT id, commit_id, project_id, name, status, value, step, experiment_name \
         FROM measurements WHERE commit_id = ANY($1) ORDER BY id",
    )
    .bind(&commit_ids)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let m_ids: Vec<i64> = measurements.iter().map(|m| m.id).collect();
    let mut tag_map = load_tags(pool, &m_ids).await?;
    let mut summary_map = load_summaries(pool, &commit_ids).await?;

    let mut measurements_by_commit: HashMap<i64, Vec<MeasurementJson>> = HashMap::new();
    for m in measurements {
        measurements_by_commit
            .entry(m.commit_id)
            .or_default()
            .push(MeasurementJson {
                id: m.id,
                name: m.name,
                status: m.status,
                value: m.value.map(|v| v.0),
                tags: tag_map.remove(&m.id).unwrap_or_default(),
                step: m.step,
                experiment_name: m.experiment_name,
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
            measurements: measurements_by_commit.remove(&c.id).unwrap_or_default(),
            summaries: summary_map.remove(&c.id).unwrap_or_default(),
        })
        .collect();

    Ok(Json(out))
}

pub async fn get_commit(
    Path(sha): Path<String>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<CommitJson>> {
    let row: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM commits WHERE sha = $1 OR short_sha = $2")
            .bind(&sha)
            .bind(&sha)
            .fetch_optional(&pool)
            .await
            .map_err(ise)?;

    match row {
        Some((id,)) => Ok(Json(commit_to_json(&pool, id).await?)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_project_commit(
    Path((project_id, sha)): Path<(i64, String)>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<CommitJson>> {
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT DISTINCT c.id FROM commits c \
         JOIN projects p ON p.repo_id = c.repo_id \
         WHERE p.id = $1 AND (c.sha = $2 OR c.short_sha = $3) \
         LIMIT 1",
    )
    .bind(project_id)
    .bind(&sha)
    .bind(&sha)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?;

    match row {
        Some((id,)) => Ok(Json(commit_to_json(&pool, id).await?)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

pub async fn get_project_github_commit(
    Path((project_id, sha)): Path<(i64, String)>,
    State(state): State<AppState>,
) -> ApiResult<Json<GithubCommitJson>> {
    let project = sqlx::query_as::<_, Project>(
        "SELECT id, repo_id, name, subdir, upstream FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let github_host = state.github_host();
    let api_base = state.api_base();
    let (owner, repo) =
        github_owner_repo(&project.upstream, &github_host).ok_or(StatusCode::BAD_REQUEST)?;
    let token = state.github_token(&owner).await?;
    let commit = get_or_fetch_github_commit(
        &state.http,
        &api_base,
        &owner,
        &repo,
        &sha,
        token.as_deref(),
    )
    .await?;
    Ok(Json(commit))
}

#[derive(serde::Deserialize)]
pub struct ScheduleCommitBody {
    sha: Option<String>,
}

pub async fn schedule_project_commit(
    Path((project_id,)): Path<(i64,)>,
    State(state): State<AppState>,
    Json(body): Json<ScheduleCommitBody>,
) -> ApiResult<(StatusCode, Json<CommitJson>)> {
    let Some(sha_raw) = body.sha else {
        return Err(StatusCode::BAD_REQUEST);
    };
    let sha = sha_raw.trim();
    if sha.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let project = sqlx::query_as::<_, Project>(
        "SELECT id, repo_id, name, subdir, upstream FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM commits WHERE repo_id = $1 AND (sha = $2 OR short_sha = $3) LIMIT 1",
    )
    .bind(project.repo_id)
    .bind(sha)
    .bind(sha)
    .fetch_optional(&state.pool)
    .await
    .map_err(ise)?;

    if let Some((id,)) = existing {
        return Ok((StatusCode::OK, Json(commit_to_json(&state.pool, id).await?)));
    }

    let github_host = state.github_host();
    let api_base = state.api_base();
    let (owner, repo) =
        github_owner_repo(&project.upstream, &github_host).ok_or(StatusCode::BAD_REQUEST)?;
    let token = state.github_token(&owner).await?;
    let gh =
        get_or_fetch_github_commit(&state.http, &api_base, &owner, &repo, sha, token.as_deref())
            .await?;

    let commit_row: (i64,) = sqlx::query_as(
        "INSERT INTO commits (repo_id, sha, short_sha, author, message, timestamp) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(project.repo_id)
    .bind(&gh.sha)
    .bind(&gh.short_sha)
    .bind(&gh.author)
    .bind(&gh.message)
    .bind(&gh.timestamp)
    .fetch_one(&state.pool)
    .await
    .map_err(ise)?;

    Ok((
        StatusCode::CREATED,
        Json(commit_to_json(&state.pool, commit_row.0).await?),
    ))
}
