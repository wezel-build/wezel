use sqlx::PgPool;
use std::time::Duration;

/// Minimum tick interval — we poll at this rate and compare against each repo's
/// `enqueue_interval_secs` to decide whether to enqueue.
const TICK_INTERVAL: Duration = Duration::from_secs(60);

/// Spawns the periodic-enqueue background task.
///
/// For each repo, checks if `enqueue_interval_secs` has elapsed since the last
/// enqueue for each (project, branch, benchmark) triple.  When it has, enqueues
/// the branch HEAD commit — deduplicating against existing pending/running jobs.
pub fn spawn(pool: PgPool) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK_INTERVAL);
        loop {
            interval.tick().await;
            if let Err(e) = tick(&pool).await {
                tracing::warn!("scheduler tick failed: {e:?}");
            }
        }
    });
}

/// One tick: iterate repos → branches → projects → benchmarks and enqueue.
async fn tick(pool: &PgPool) -> sqlx::Result<()> {
    // Fetch repos whose interval has elapsed since their most-recent enqueue
    // (or that have never had anything enqueued).
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

/// For a single repo, enqueue branch-head jobs for every (project, branch, benchmark).
async fn enqueue_repo(pool: &PgPool, repo_id: i64) -> sqlx::Result<()> {
    // Get all branches for this repo.
    let branches: Vec<(String, String)> = sqlx::query_as(
        "SELECT name, head_sha FROM branches WHERE repo_id = $1",
    )
    .bind(repo_id)
    .fetch_all(pool)
    .await?;

    if branches.is_empty() {
        return Ok(());
    }

    // Get all projects for this repo.
    let projects: Vec<(i64,)> =
        sqlx::query_as("SELECT id FROM projects WHERE repo_id = $1")
            .bind(repo_id)
            .fetch_all(pool)
            .await?;

    for (project_id,) in &projects {
        // Discover known benchmarks for this project.
        let benchmarks: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT benchmark_name FROM (
                 SELECT benchmark_name FROM forager_queue WHERE project_id = $1
                 UNION
                 SELECT ft.benchmark_name FROM forager_tokens ft
                 JOIN commits c ON ft.commit_id = c.id
                 JOIN projects p ON p.repo_id = c.repo_id
                 WHERE p.id = $1
             ) t",
        )
        .bind(project_id)
        .fetch_all(pool)
        .await?;

        for (branch_name, head_sha) in &branches {
            for (benchmark_name,) in &benchmarks {
                enqueue_if_needed(pool, *project_id, head_sha, benchmark_name, branch_name)
                    .await?;
            }
        }
    }

    Ok(())
}

/// Insert a queue entry unless a pending/running job already exists for this
/// (project, sha, benchmark) triple.
async fn enqueue_if_needed(
    pool: &PgPool,
    project_id: i64,
    commit_sha: &str,
    benchmark_name: &str,
    branch: &str,
) -> sqlx::Result<()> {
    let inserted: Option<(i64,)> = sqlx::query_as(
        "INSERT INTO forager_queue (project_id, commit_sha, benchmark_name, branch)
         SELECT $1, $2, $3, $4
         WHERE NOT EXISTS (
             SELECT 1 FROM forager_queue
             WHERE project_id = $1 AND commit_sha = $2 AND benchmark_name = $3
               AND status IN ('pending', 'running')
         )
         RETURNING id",
    )
    .bind(project_id)
    .bind(commit_sha)
    .bind(benchmark_name)
    .bind(branch)
    .fetch_optional(pool)
    .await?;

    if let Some((id,)) = inserted {
        tracing::info!(
            id,
            project_id,
            commit_sha,
            benchmark_name,
            branch,
            "scheduler: enqueued job"
        );
    }

    Ok(())
}
