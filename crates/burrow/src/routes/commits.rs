use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use reqwest::Client;
use sqlx::PgPool;

use crate::github::{get_or_fetch_github_commit, github_owner_repo};
use crate::models::*;
use crate::{ApiResult, ise};

// ── Helper ────────────────────────────────────────────────────────────────────

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
        "SELECT id, commit_id, project_id, name, kind, status, value, unit, step \
         FROM measurements WHERE commit_id = $1 ORDER BY id",
    )
    .bind(commit_id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let m_ids: Vec<i64> = measurements.iter().map(|m| m.id).collect();

    let details = sqlx::query_as::<_, MeasurementDetail>(
        "SELECT measurement_id, name, value \
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
        measurements: measurements_json,
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
    // Get commits that have measurements for this project.
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
        "SELECT id, commit_id, project_id, name, kind, status, value, unit, step \
         FROM measurements WHERE commit_id = ANY($1) ORDER BY id",
    )
    .bind(&commit_ids)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let m_ids: Vec<i64> = measurements.iter().map(|m| m.id).collect();

    let details = sqlx::query_as::<_, MeasurementDetail>(
        "SELECT measurement_id, name, value \
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
            measurements: measurements_by_commit.remove(&c.id).unwrap_or_default(),
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
    // Find commit by SHA, but only if it has measurements for this project.
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
    State(pool): State<PgPool>,
) -> ApiResult<Json<GithubCommitJson>> {
    let project = sqlx::query_as::<_, Project>(
        "SELECT id, repo_id, name, subdir, upstream FROM projects WHERE id = $1",
    )
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

#[derive(serde::Deserialize)]
pub struct ScheduleCommitBody {
    sha: Option<String>,
}

pub async fn schedule_project_commit(
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

    let project = sqlx::query_as::<_, Project>(
        "SELECT id, repo_id, name, subdir, upstream FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM commits WHERE repo_id = $1 AND (sha = $2 OR short_sha = $3) LIMIT 1",
    )
    .bind(project.repo_id)
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
        "INSERT INTO commits (repo_id, sha, short_sha, author, message, timestamp) \
         VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
    )
    .bind(project.repo_id)
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
