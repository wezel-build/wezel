use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::models::{ForagerQueueJobStatus, IdRow};
use crate::{ApiResult, ise};

#[derive(Deserialize)]
pub struct ForagerClaimBody {
    project_upstream: String,
    commit_sha: String,
    benchmark_name: String,
    // Optional commit metadata for when GitHub API is not available.
    commit_author: Option<String>,
    commit_message: Option<String>,
    commit_timestamp: Option<String>,
}

pub async fn post_forager_claim(
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
    let commit_id: i64 = match sqlx::query_as::<_, (i64,)>(
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
            let message = body.commit_message.as_deref().unwrap_or("").to_string();
            let timestamp = body.commit_timestamp.as_deref().unwrap_or("").to_string();
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

pub async fn post_forager_run(
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
    let (project_id,): (i64,) = sqlx::query_as("SELECT project_id FROM commits WHERE id = $1")
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

// ── Job queue ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ForagerEnqueueBody {
    project_upstream: String,
    commit_sha: String,
    benchmark_name: String,
}

pub async fn post_forager_jobs(
    State(pool): State<PgPool>,
    Json(body): Json<ForagerEnqueueBody>,
) -> ApiResult<(StatusCode, Json<ForagerQueueJobStatus>)> {
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

    // Return existing pending/running job if one already exists.
    if let Some((id, status)) = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, status FROM forager_queue \
         WHERE project_id = $1 AND commit_sha = $2 AND benchmark_name = $3 \
         AND status IN ('pending', 'running') \
         LIMIT 1",
    )
    .bind(project_id)
    .bind(sha)
    .bind(benchmark_name)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?
    {
        return Ok((
            StatusCode::CREATED,
            Json(ForagerQueueJobStatus { id, status }),
        ));
    }

    let (id,): (i64,) = sqlx::query_as(
        "INSERT INTO forager_queue (project_id, commit_sha, benchmark_name) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(project_id)
    .bind(sha)
    .bind(benchmark_name)
    .fetch_one(&pool)
    .await
    .map_err(ise)?;

    Ok((
        StatusCode::CREATED,
        Json(ForagerQueueJobStatus {
            id,
            status: "pending".to_string(),
        }),
    ))
}

#[derive(Deserialize)]
pub struct ForagerJobsNextBody {
    project_upstream: String,
}

pub async fn post_forager_jobs_next(
    State(pool): State<PgPool>,
    Json(body): Json<ForagerJobsNextBody>,
) -> ApiResult<(StatusCode, Json<Option<wezel_types::ForagerQueueJob>>)> {
    let upstream = body.project_upstream.trim();
    if upstream.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let row = sqlx::query_as::<_, (i64, i64, String, String, String)>(
        "UPDATE forager_queue fq \
         SET status = 'running', claimed_at = now() \
         WHERE fq.id = ( \
             SELECT fq2.id FROM forager_queue fq2 \
             JOIN projects p ON fq2.project_id = p.id \
             WHERE p.upstream = $1 AND fq2.status = 'pending' \
             ORDER BY fq2.id ASC \
             LIMIT 1 \
             FOR UPDATE SKIP LOCKED \
         ) \
         RETURNING fq.id, fq.project_id, fq.commit_sha, fq.benchmark_name, \
                   (SELECT upstream FROM projects WHERE id = fq.project_id)",
    )
    .bind(upstream)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?;

    match row {
        None => Ok((StatusCode::NO_CONTENT, Json(None))),
        Some((id, project_id, commit_sha, benchmark_name, project_upstream)) => Ok((
            StatusCode::OK,
            Json(Some(wezel_types::ForagerQueueJob {
                id: id as u64,
                project_id: project_id as u64,
                project_upstream,
                commit_sha,
                benchmark_name,
            })),
        )),
    }
}

#[derive(Deserialize)]
pub struct ForagerJobPatchBody {
    status: String,
    error: Option<String>,
}

pub async fn patch_forager_job(
    State(pool): State<PgPool>,
    Path(id): Path<i64>,
    Json(body): Json<ForagerJobPatchBody>,
) -> ApiResult<StatusCode> {
    let status = body.status.trim();
    if status != "complete" && status != "failed" {
        return Err(StatusCode::BAD_REQUEST);
    }

    let rows_affected = sqlx::query(
        "UPDATE forager_queue \
         SET status = $1, completed_at = now(), error_text = $2 \
         WHERE id = $3",
    )
    .bind(status)
    .bind(body.error.as_deref())
    .bind(id)
    .execute(&pool)
    .await
    .map_err(ise)?
    .rows_affected();

    if rows_affected == 0 {
        return Err(StatusCode::NOT_FOUND);
    }

    Ok(StatusCode::OK)
}

pub async fn get_project_benchmarks(
    Path(project_id): Path<i64>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<Vec<String>>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT benchmark_name FROM (
             SELECT benchmark_name FROM forager_queue WHERE project_id = $1
             UNION
             SELECT ft.benchmark_name FROM forager_tokens ft
             JOIN commits c ON ft.commit_id = c.id
             WHERE c.project_id = $1
         ) t ORDER BY benchmark_name",
    )
    .bind(project_id)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;
    Ok(Json(rows.into_iter().map(|(n,)| n).collect()))
}
