use std::time::Duration;

use crate::AppState;

const TICK_INTERVAL: Duration = Duration::from_secs(60);

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK_INTERVAL);
        loop {
            interval.tick().await;
            if let Err(e) = tick(&state.pool).await {
                tracing::warn!("scheduler tick failed: {e:?}");
            }
            if let Err(e) = crate::routes::tools::refresh_tool_release(&state).await {
                tracing::debug!("tool release refresh: {e}");
            }
        }
    });
}

async fn tick(pool: &sqlx::PgPool) -> sqlx::Result<()> {
    let repos: Vec<(i64, i32)> = sqlx::query_as(
        "SELECT r.id, r.enqueue_interval_secs
         FROM repos r
         WHERE NOT EXISTS (
             SELECT 1 FROM forager_queue fq
             JOIN projects p ON fq.project_id = p.id
             WHERE p.repo_id = r.id
               AND fq.created_at > now() - make_interval(secs => r.enqueue_interval_secs::double precision)
         )",
    )
    .fetch_all(pool)
    .await?;

    if repos.is_empty() {
        return Ok(());
    }

    for (repo_id, _interval_secs) in &repos {
        enqueue_repo(pool, *repo_id).await?;
    }

    Ok(())
}

async fn enqueue_repo(pool: &sqlx::PgPool, repo_id: i64) -> sqlx::Result<()> {
    let branches: Vec<(String, String)> =
        sqlx::query_as("SELECT name, head_sha FROM branches WHERE repo_id = $1")
            .bind(repo_id)
            .fetch_all(pool)
            .await?;

    if branches.is_empty() {
        return Ok(());
    }

    let projects: Vec<(i64,)> = sqlx::query_as("SELECT id FROM projects WHERE repo_id = $1")
        .bind(repo_id)
        .fetch_all(pool)
        .await?;

    for (project_id,) in &projects {
        let experiments: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT experiment_name FROM (
                 SELECT experiment_name FROM forager_queue WHERE project_id = $1
                 UNION
                 SELECT ft.experiment_name FROM forager_tokens ft
                 JOIN commits c ON ft.commit_id = c.id
                 JOIN projects p ON p.repo_id = c.repo_id
                 WHERE p.id = $1
             ) t",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;

        for (branch_name, head_sha) in &branches {
            for (experiment_name,) in &experiments {
                enqueue_if_needed(pool, *project_id, head_sha, experiment_name, branch_name)
                    .await?;
            }
        }
    }

    Ok(())
}

async fn enqueue_if_needed(
    pool: &sqlx::PgPool,
    project_id: i64,
    commit_sha: &str,
    experiment_name: &str,
    branch: &str,
) -> sqlx::Result<()> {
    let inserted: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO forager_queue (project_id, commit_sha, experiment_name, branch)
         SELECT $1, $2, $3, $4
         WHERE NOT EXISTS (
             SELECT 1 FROM forager_queue
             WHERE project_id = $1 AND commit_sha = $2 AND experiment_name = $3
               AND status IN ('pending', 'running')
         )
         RETURNING id",
    )
    .bind(project_id)
    .bind(commit_sha)
    .bind(experiment_name)
    .bind(branch)
    .fetch_optional(pool)
    .await?;

    if let Some((id,)) = inserted {
        tracing::info!(
            id,
            project_id,
            commit_sha,
            experiment_name,
            branch,
            "scheduler: enqueued job"
        );
    }

    Ok(())
}
