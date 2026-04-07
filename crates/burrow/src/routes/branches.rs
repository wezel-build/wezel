use std::collections::HashMap;

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::models::*;
use crate::{ApiResult, ise};

// ── Query params ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TimelineQuery {
    limit: Option<i32>,
}

#[derive(Deserialize)]
pub struct CompareQuery {
    pub base_sha: String,
    pub head_sha: String,
}

// ── Response types ───────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct BranchTimeline {
    pub branch: String,
    pub commits: Vec<CommitJson>,
}

#[derive(Serialize)]
pub struct CompareResponse {
    pub base: CommitJson,
    pub head: CommitJson,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Batch-fetch measurements (scoped to a single project) for a list of commits,
/// then assemble them into `CommitJson` values preserving input order.
async fn build_project_commits(
    commits: Vec<Commit>,
    project_id: i64,
    pool: &PgPool,
) -> ApiResult<Vec<CommitJson>> {
    if commits.is_empty() {
        return Ok(vec![]);
    }

    let commit_ids: Vec<i64> = commits.iter().map(|c| c.id).collect();

    let measurements = sqlx::query_as::<_, Measurement>(
        "SELECT id, commit_id, project_id, name, status, value, step \
         FROM measurements WHERE commit_id = ANY($1) AND project_id = $2 ORDER BY id",
    )
    .bind(&commit_ids)
    .bind(project_id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    let m_ids: Vec<i64> = measurements.iter().map(|m| m.id).collect();
    let mut tag_map = crate::routes::commits::load_tags(pool, &m_ids).await?;

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
            });
    }

    Ok(commits
        .into_iter()
        .map(|c| CommitJson {
            sha: c.sha,
            short_sha: c.short_sha,
            author: c.author,
            message: c.message,
            timestamp: c.timestamp,
            measurements: measurements_by_commit.remove(&c.id).unwrap_or_default(),
        })
        .collect())
}

// ── Handlers ─────────────────────────────────────────────────────────────────

/// Walk the first-parent chain from a branch HEAD and return commits with
/// their measurements for this project.  Depth is capped by `?limit` (default 50).
pub async fn get_branch_timeline(
    Path((project_id, branch)): Path<(i64, String)>,
    Query(query): Query<TimelineQuery>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<BranchTimeline>> {
    let limit = query.limit.unwrap_or(50).clamp(1, 500);

    let project = sqlx::query_as::<_, Project>(
        "SELECT id, repo_id, name, subdir, upstream FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let commits = sqlx::query_as::<_, Commit>(
        "WITH RECURSIVE chain AS ( \
            SELECT c.id, c.repo_id, c.sha, c.short_sha, c.parent_sha, \
                   c.author, c.message, c.timestamp, 0 AS depth \
            FROM commits c \
            JOIN branches b ON b.repo_id = c.repo_id AND b.head_sha = c.sha \
            WHERE c.repo_id = $1 AND b.name = $2 \
          UNION ALL \
            SELECT c.id, c.repo_id, c.sha, c.short_sha, c.parent_sha, \
                   c.author, c.message, c.timestamp, ch.depth + 1 \
            FROM commits c \
            JOIN chain ch ON c.sha = ch.parent_sha AND c.repo_id = $1 \
            WHERE ch.depth < $3 \
        ) \
        SELECT id, repo_id, sha, short_sha, parent_sha, author, message, timestamp \
        FROM chain ORDER BY depth",
    )
    .bind(project.repo_id)
    .bind(&branch)
    .bind(limit - 1)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    let commits = build_project_commits(commits, project_id, &pool).await?;

    Ok(Json(BranchTimeline { branch, commits }))
}

/// Return measurements for two commits side-by-side.  Frontend computes deltas.
pub async fn get_project_compare(
    Path((project_id,)): Path<(i64,)>,
    Query(query): Query<CompareQuery>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<CompareResponse>> {
    let project = sqlx::query_as::<_, Project>(
        "SELECT id, repo_id, name, subdir, upstream FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let base_commit = sqlx::query_as::<_, Commit>(
        "SELECT id, repo_id, sha, short_sha, parent_sha, author, message, timestamp \
         FROM commits WHERE repo_id = $1 AND (sha = $2 OR short_sha = $2) LIMIT 1",
    )
    .bind(project.repo_id)
    .bind(&query.base_sha)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let head_commit = sqlx::query_as::<_, Commit>(
        "SELECT id, repo_id, sha, short_sha, parent_sha, author, message, timestamp \
         FROM commits WHERE repo_id = $1 AND (sha = $2 OR short_sha = $2) LIMIT 1",
    )
    .bind(project.repo_id)
    .bind(&query.head_sha)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let mut json = build_project_commits(vec![base_commit, head_commit], project_id, &pool).await?;
    let head = json.pop().unwrap();
    let base = json.pop().unwrap();

    Ok(Json(CompareResponse { base, head }))
}
