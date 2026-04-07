use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::models::{ForagerQueueJobStatus, IdRow};
use crate::{ApiResult, ise};

/// Find or create a repo + project from an upstream URL, returning (repo_id, project_id).
async fn find_or_create_project(pool: &PgPool, upstream: &str) -> Result<(i64, i64), StatusCode> {
    let upstream = &crate::github::normalize_upstream(upstream);
    let project_name = upstream.rsplit('/').next().unwrap_or(upstream);

    // Find or create repo.
    let repo_id: i64 = match sqlx::query_as::<_, (i64,)>("SELECT id FROM repos WHERE upstream = $1")
        .bind(upstream)
        .fetch_optional(pool)
        .await
        .map_err(ise)?
    {
        Some((id,)) => id,
        None => {
            sqlx::query_as::<_, IdRow>("INSERT INTO repos (upstream) VALUES ($1) RETURNING id")
                .bind(upstream)
                .fetch_one(pool)
                .await
                .map_err(ise)?
                .id
        }
    };

    // Find or create project.
    let project_id: i64 =
        match sqlx::query_as::<_, (i64,)>(
            "SELECT id FROM projects WHERE repo_id = $1 AND subdir = ''",
        )
        .bind(repo_id)
        .fetch_optional(pool)
        .await
        .map_err(ise)?
        {
            Some((id,)) => id,
            None => sqlx::query_as::<_, IdRow>(
                "INSERT INTO projects (repo_id, name, upstream) VALUES ($1, $2, $3) RETURNING id",
            )
            .bind(repo_id)
            .bind(project_name)
            .bind(upstream)
            .fetch_one(pool)
            .await
            .map_err(ise)?
            .id,
        };

    Ok((repo_id, project_id))
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

    // Get repo_id → project_id for measurements.
    let (repo_id,): (i64,) = sqlx::query_as("SELECT repo_id FROM commits WHERE id = $1")
        .bind(commit_id)
        .fetch_one(&pool)
        .await
        .map_err(ise)?;

    // Find the default project for this repo (subdir = '').
    let (project_id,): (i64,) =
        sqlx::query_as("SELECT id FROM projects WHERE repo_id = $1 AND subdir = '' LIMIT 1")
            .bind(repo_id)
            .fetch_one(&pool)
            .await
            .map_err(ise)?;

    // Get the commit SHA for later use.
    let (commit_sha,): (String,) = sqlx::query_as("SELECT sha FROM commits WHERE id = $1")
        .bind(commit_id)
        .fetch_one(&pool)
        .await
        .map_err(ise)?;

    // Collect all measurements across steps for conclusion computation.
    let all_measurements: Vec<wezel_types::ForagerPluginOutput> = body
        .steps
        .iter()
        .flat_map(|s| s.measurements.iter().cloned())
        .collect();

    for step_report in &body.steps {
        for m in &step_report.measurements {
            let (measurement_id,): (i64,) = sqlx::query_as(
                "INSERT INTO measurements \
                 (commit_id, project_id, name, status, value, step) \
                 VALUES ($1, $2, $3, 'complete', $4, $5) RETURNING id",
            )
            .bind(commit_id)
            .bind(project_id)
            .bind(&m.name)
            .bind(sqlx::types::Json(&m.value))
            .bind(&step_report.step)
            .fetch_one(&pool)
            .await
            .map_err(ise)?;

            for (key, value) in &m.tags {
                sqlx::query(
                    "INSERT INTO measurement_tags (measurement_id, key, value) \
                     VALUES ($1, $2, $3)",
                )
                .bind(measurement_id)
                .bind(key)
                .bind(value)
                .execute(&pool)
                .await
                .map_err(ise)?;
            }
        }
    }

    // Get the experiment_name from the token row.
    let (experiment_name,): (String,) =
        sqlx::query_as("SELECT experiment_name FROM forager_tokens WHERE token = $1")
            .bind(&body.token)
            .fetch_one(&pool)
            .await
            .map_err(ise)?;

    // Compute and store summary values.
    let mut computed: Vec<(String, f64)> = Vec::new();
    for def in &body.summaries {
        if let Some(value) = def.compute(&all_measurements) {
            sqlx::query(
                "INSERT INTO summary_values \
                 (project_id, experiment_name, commit_id, name, value) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (project_id, experiment_name, commit_id, name) \
                 DO UPDATE SET value = EXCLUDED.value, computed_at = now()",
            )
            .bind(project_id)
            .bind(&experiment_name)
            .bind(commit_id)
            .bind(&def.name)
            .bind(value)
            .execute(&pool)
            .await
            .map_err(ise)?;

            computed.push((def.name.clone(), value));
        }
    }

    if let Some(bisection_id) = body.bisection_id {
        // ── Bisection progression ────────────────────────────────────────
        progress_bisection(
            &pool,
            bisection_id as i64,
            repo_id,
            commit_id,
            &experiment_name,
        )
        .await?;
    } else {
        // ── Regression detection (normal runs only) ──────────────────────
        detect_regressions(
            &pool,
            repo_id,
            project_id,
            commit_id,
            &commit_sha,
            &experiment_name,
            &body.summaries,
            &computed,
        )
        .await?;
    }

    Ok(StatusCode::OK)
}

/// After storing conclusion values for a normal run, compare each bisect-eligible
/// conclusion against recent history. If the regression detector fires, create a
/// bisection and enqueue the midpoint.
async fn detect_regressions(
    pool: &PgPool,
    repo_id: i64,
    project_id: i64,
    commit_id: i64,
    commit_sha: &str,
    experiment_name: &str,
    conclusion_defs: &[wezel_types::SummaryDef],
    computed: &[(String, f64)],
) -> Result<(), StatusCode> {
    if computed.is_empty() {
        return Ok(());
    }

    let detector = crate::regression::detector();
    let history_len = detector.history_len();

    let ancestor_ids: Vec<(i64,)> = sqlx::query_as(
        "WITH RECURSIVE chain AS ( \
             SELECT parent_sha, 0 AS depth FROM commits WHERE id = $1 \
           UNION ALL \
             SELECT c.parent_sha, ch.depth + 1 \
             FROM commits c \
             JOIN chain ch ON c.sha = ch.parent_sha AND c.repo_id = $2 \
             WHERE ch.depth < $3 AND ch.parent_sha IS NOT NULL \
         ) \
         SELECT c.id FROM chain ch \
         JOIN commits c ON c.sha = ch.parent_sha AND c.repo_id = $2 \
         ORDER BY ch.depth",
    )
    .bind(commit_id)
    .bind(repo_id)
    .bind(history_len as i64)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    if ancestor_ids.is_empty() {
        return Ok(());
    }

    let parent_sha: Option<String> =
        sqlx::query_as::<_, (Option<String>,)>("SELECT parent_sha FROM commits WHERE id = $1")
            .bind(commit_id)
            .fetch_one(pool)
            .await
            .map_err(ise)?
            .0;

    let Some(parent_sha) = parent_sha else {
        return Ok(());
    };

    let branch: String = sqlx::query_as::<_, (String,)>(
        "SELECT branch FROM forager_queue \
         WHERE project_id = $1 AND commit_sha = $2 \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(project_id)
    .bind(commit_sha)
    .fetch_optional(pool)
    .await
    .map_err(ise)?
    .map(|(b,)| b)
    .unwrap_or_else(|| "main".to_string());

    let ancestor_commit_ids: Vec<i64> = ancestor_ids.into_iter().map(|(id,)| id).collect();

    for (conclusion_name, new_value) in computed {
        let new_value = *new_value;
        let bisect = conclusion_defs
            .iter()
            .find(|d| &d.name == conclusion_name)
            .map(|d| d.bisect)
            .unwrap_or(true);

        if !bisect {
            continue;
        }

        let history: Vec<(f64,)> = sqlx::query_as(
            "SELECT value FROM summary_values \
             WHERE project_id = $1 AND experiment_name = $2 AND name = $3 \
               AND commit_id = ANY($4) \
             ORDER BY commit_id ASC",
        )
        .bind(project_id)
        .bind(experiment_name)
        .bind(conclusion_name)
        .bind(&ancestor_commit_ids)
        .fetch_all(pool)
        .await
        .map_err(ise)?;

        let history_values: Vec<f64> = history.into_iter().map(|(v,)| v).collect();

        if !detector.is_regression(&history_values, new_value) {
            continue;
        }

        let existing: Option<(i64,)> = sqlx::query_as(
            "SELECT id FROM bisections \
             WHERE project_id = $1 AND experiment_name = $2 AND measurement_name = $3 \
               AND status = 'active' \
             LIMIT 1",
        )
        .bind(project_id)
        .bind(experiment_name)
        .bind(conclusion_name)
        .fetch_optional(pool)
        .await
        .map_err(ise)?;

        if existing.is_some() {
            continue;
        }

        let baseline = history_values.last().copied().unwrap_or(0.0);
        tracing::info!(
            "regression detected: {conclusion_name} ({baseline} -> {new_value}), \
             bisecting {parent_sha}..{commit_sha}",
        );

        let (bisection_id,): (i64,) = sqlx::query_as(
            "INSERT INTO bisections \
             (project_id, experiment_name, measurement_name, branch, \
              good_sha, bad_sha, good_value, bad_value, identity_tags) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::jsonb) RETURNING id",
        )
        .bind(project_id)
        .bind(experiment_name)
        .bind(conclusion_name)
        .bind(&branch)
        .bind(&parent_sha)
        .bind(commit_sha)
        .bind(baseline)
        .bind(new_value)
        .bind("{}")
        .fetch_one(pool)
        .await
        .map_err(ise)?;

        enqueue_midpoint(
            pool,
            bisection_id,
            project_id,
            repo_id,
            experiment_name,
            &parent_sha,
            commit_sha,
        )
        .await?;
    }

    Ok(())
}

/// Progress a bisection after receiving measurement results for a midpoint commit.
async fn progress_bisection(
    pool: &PgPool,
    bisection_id: i64,
    repo_id: i64,
    commit_id: i64,
    experiment_name: &str,
) -> Result<(), StatusCode> {
    let row: Option<(i64, String, String, String, f64, f64, String)> = sqlx::query_as(
        "SELECT project_id, measurement_name, good_sha, bad_sha, \
                good_value, bad_value, status \
         FROM bisections WHERE id = $1",
    )
    .bind(bisection_id)
    .fetch_optional(pool)
    .await
    .map_err(ise)?;

    let (project_id, conclusion_name, good_sha, bad_sha, good_value, bad_value, status) =
        row.ok_or(StatusCode::NOT_FOUND)?;

    if status != "active" {
        return Ok(());
    }

    // Look up the summary value for this commit.
    let value_row: Option<(f64,)> = sqlx::query_as(
        "SELECT value FROM summary_values \
         WHERE project_id = $1 AND experiment_name = $2 AND commit_id = $3 AND name = $4",
    )
    .bind(project_id)
    .bind(experiment_name)
    .bind(commit_id)
    .bind(&conclusion_name)
    .fetch_optional(pool)
    .await
    .map_err(ise)?;

    let Some((value,)) = value_row else {
        tracing::warn!(
            "bisection {bisection_id}: no conclusion value '{conclusion_name}' for commit {commit_id}"
        );
        return Ok(());
    };

    let detector = crate::regression::detector();
    let is_good = detector.is_good(good_value, bad_value, value);

    let (tested_sha,): (String,) = sqlx::query_as("SELECT sha FROM commits WHERE id = $1")
        .bind(commit_id)
        .fetch_one(pool)
        .await
        .map_err(ise)?;

    let (new_good_sha, new_bad_sha) = if is_good {
        (tested_sha.clone(), bad_sha.clone())
    } else {
        (good_sha.clone(), tested_sha.clone())
    };

    tracing::info!(
        "bisection {bisection_id}: {tested_sha} classified as {} \
         (value={value}, good={good_value}, bad={bad_value})",
        if is_good { "good" } else { "bad" },
    );

    let adjacent = are_adjacent(pool, repo_id, &new_good_sha, &new_bad_sha).await?;

    if adjacent {
        sqlx::query(
            "UPDATE bisections \
             SET good_sha = $2, bad_sha = $3, status = 'complete', \
                 culprit_sha = $3, completed_at = now() \
             WHERE id = $1",
        )
        .bind(bisection_id)
        .bind(&new_good_sha)
        .bind(&new_bad_sha)
        .execute(pool)
        .await
        .map_err(ise)?;

        tracing::info!("bisection {bisection_id} complete: culprit is {new_bad_sha}");
    } else {
        sqlx::query("UPDATE bisections SET good_sha = $2, bad_sha = $3 WHERE id = $1")
            .bind(bisection_id)
            .bind(&new_good_sha)
            .bind(&new_bad_sha)
            .execute(pool)
            .await
            .map_err(ise)?;

        // Get experiment_name from the bisection row for the queue entry.
        let (exp_name,): (String,) =
            sqlx::query_as("SELECT experiment_name FROM bisections WHERE id = $1")
                .bind(bisection_id)
                .fetch_one(pool)
                .await
                .map_err(ise)?;

        enqueue_midpoint(
            pool,
            bisection_id,
            project_id,
            repo_id,
            &exp_name,
            &new_good_sha,
            &new_bad_sha,
        )
        .await?;
    }

    Ok(())
}

/// Check if good_sha is the direct parent of bad_sha.
async fn are_adjacent(
    pool: &PgPool,
    repo_id: i64,
    good_sha: &str,
    bad_sha: &str,
) -> Result<bool, StatusCode> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT parent_sha FROM commits WHERE repo_id = $1 AND sha = $2")
            .bind(repo_id)
            .bind(bad_sha)
            .fetch_optional(pool)
            .await
            .map_err(ise)?;

    Ok(row.is_some_and(|(parent,)| parent == good_sha))
}

/// Walk the commit chain from bad_sha to good_sha via recursive CTE,
/// pick the midpoint, and enqueue it.
async fn enqueue_midpoint(
    pool: &PgPool,
    bisection_id: i64,
    project_id: i64,
    repo_id: i64,
    experiment_name: &str,
    good_sha: &str,
    bad_sha: &str,
) -> Result<(), StatusCode> {
    // Walk the chain from bad back to good.
    let chain: Vec<(String, i32)> = sqlx::query_as(
        "WITH RECURSIVE chain AS ( \
             SELECT sha, parent_sha, 0 AS depth \
             FROM commits WHERE sha = $1 AND repo_id = $3 \
           UNION ALL \
             SELECT c.sha, c.parent_sha, ch.depth + 1 \
             FROM commits c \
             JOIN chain ch ON c.sha = ch.parent_sha AND c.repo_id = $3 \
             WHERE ch.sha != $2 \
         ) \
         SELECT sha, depth FROM chain ORDER BY depth",
    )
    .bind(bad_sha)
    .bind(good_sha)
    .bind(repo_id)
    .fetch_all(pool)
    .await
    .map_err(ise)?;

    if chain.len() <= 2 {
        // Adjacent or missing — nothing to bisect further.
        return Ok(());
    }

    // Pick the midpoint (skip first which is bad_sha itself).
    let mid_idx = chain.len() / 2;
    let mid_sha = &chain[mid_idx].0;

    tracing::info!(
        "bisection {bisection_id}: enqueuing midpoint {mid_sha} (chain len={})",
        chain.len()
    );

    // Enqueue with bisection_id.
    sqlx::query(
        "INSERT INTO forager_queue (project_id, commit_sha, experiment_name, bisection_id) \
         VALUES ($1, $2, $3, $4)",
    )
    .bind(project_id)
    .bind(mid_sha)
    .bind(experiment_name)
    .bind(bisection_id)
    .execute(pool)
    .await
    .map_err(ise)?;

    Ok(())
}

// ── Job queue ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ForagerEnqueueBody {
    project_upstream: String,
    commit_sha: String,
    experiment_name: String,
}

pub async fn post_forager_jobs(
    State(pool): State<PgPool>,
    Json(body): Json<ForagerEnqueueBody>,
) -> ApiResult<(StatusCode, Json<ForagerQueueJobStatus>)> {
    let upstream = body.project_upstream.trim();
    let sha = body.commit_sha.trim();
    let experiment_name = body.experiment_name.trim();

    if upstream.is_empty() || sha.is_empty() || experiment_name.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let (_repo_id, project_id) = find_or_create_project(&pool, upstream).await?;

    // Return existing pending/running job if one already exists.
    if let Some((id, status)) = sqlx::query_as::<_, (i64, String)>(
        "SELECT id, status FROM forager_queue \
         WHERE project_id = $1 AND commit_sha = $2 AND experiment_name = $3 \
         AND status IN ('pending', 'running') \
         LIMIT 1",
    )
    .bind(project_id)
    .bind(sha)
    .bind(experiment_name)
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
        "INSERT INTO forager_queue (project_id, commit_sha, experiment_name) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(project_id)
    .bind(sha)
    .bind(experiment_name)
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
) -> ApiResult<(StatusCode, Json<Option<wezel_types::ForagerJob>>)> {
    let upstream = body.project_upstream.trim();
    if upstream.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    // Atomically claim the next pending job.
    let row = sqlx::query_as::<_, (i64, i64, String, String, String, Option<i64>)>(
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
         RETURNING fq.id, fq.project_id, fq.commit_sha, fq.experiment_name, \
                   (SELECT upstream FROM projects WHERE id = fq.project_id), \
                   fq.bisection_id",
    )
    .bind(upstream)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?;

    let Some((job_id, project_id, commit_sha, experiment_name, project_upstream, bisection_id)) =
        row
    else {
        return Ok((StatusCode::NO_CONTENT, Json(None)));
    };

    // Look up the commit — it must already exist (created by webhook).
    let (repo_id,): (i64,) = sqlx::query_as("SELECT repo_id FROM projects WHERE id = $1")
        .bind(project_id)
        .fetch_one(&pool)
        .await
        .map_err(ise)?;

    let (commit_id,): (i64,) =
        sqlx::query_as("SELECT id FROM commits WHERE repo_id = $1 AND sha = $2")
            .bind(repo_id)
            .bind(&commit_sha)
            .fetch_one(&pool)
            .await
            .map_err(ise)?;

    // Create forager token (expires in 4 hours).
    let token = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO forager_tokens (commit_id, experiment_name, token, expires_at) \
         VALUES ($1, $2, $3, now() + interval '4 hours')",
    )
    .bind(commit_id)
    .bind(&experiment_name)
    .bind(&token)
    .execute(&pool)
    .await
    .map_err(ise)?;

    Ok((
        StatusCode::OK,
        Json(Some(wezel_types::ForagerJob {
            id: job_id as u64,
            token,
            commit_sha,
            project_id: project_id as u64,
            project_upstream,
            experiment_name,
            bisection_id: bisection_id.map(|id| id as u64),
        })),
    ))
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

pub async fn get_project_experiments(
    Path(project_id): Path<i64>,
    State(pool): State<PgPool>,
) -> ApiResult<Json<Vec<String>>> {
    let rows: Vec<(String,)> = sqlx::query_as(
        "SELECT DISTINCT experiment_name FROM (
             SELECT experiment_name FROM forager_queue WHERE project_id = $1
             UNION
             SELECT ft.experiment_name FROM forager_tokens ft
             JOIN commits c ON ft.commit_id = c.id
             JOIN projects p ON p.repo_id = c.repo_id
             WHERE p.id = $1
         ) t ORDER BY experiment_name",
    )
    .bind(project_id)
    .fetch_all(&pool)
    .await
    .map_err(ise)?;
    Ok(Json(rows.into_iter().map(|(n,)| n).collect()))
}
