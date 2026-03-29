use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use reqwest::Client;
use serde_json::Value;
use sqlx::PgPool;

use crate::github::{github_api, github_owner_repo};
use crate::models::*;
use crate::{ApiResult, ise};

pub async fn create_project(
    State(pool): State<PgPool>,
    Json(body): Json<Value>,
) -> ApiResult<(StatusCode, Json<Project>)> {
    let name = body["name"].as_str().ok_or(StatusCode::BAD_REQUEST)?;
    let upstream = body["upstream"].as_str().ok_or(StatusCode::BAD_REQUEST)?;

    // Find or create repo.
    let repo_id: i64 = match sqlx::query_as::<_, (i64,)>("SELECT id FROM repos WHERE upstream = $1")
        .bind(upstream)
        .fetch_optional(&pool)
        .await
        .map_err(ise)?
    {
        Some((id,)) => id,
        None => {
            sqlx::query_as::<_, IdRow>("INSERT INTO repos (upstream) VALUES ($1) RETURNING id")
                .bind(upstream)
                .fetch_one(&pool)
                .await
                .map_err(ise)?
                .id
        }
    };

    let project = sqlx::query_as::<_, Project>(
        "INSERT INTO projects (repo_id, name, upstream) \
         VALUES ($1, $2, $3) \
         RETURNING id, repo_id, name, subdir, upstream",
    )
    .bind(repo_id)
    .bind(name)
    .bind(upstream)
    .fetch_one(&pool)
    .await
    .map_err(ise)?;

    Ok((StatusCode::CREATED, Json(project)))
}

pub async fn get_projects(State(pool): State<PgPool>) -> ApiResult<Json<Vec<Project>>> {
    let projects = sqlx::query_as::<_, Project>(
        "SELECT id, repo_id, name, subdir, upstream FROM projects ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .map_err(ise)?;
    Ok(Json(projects))
}

pub async fn rename_project(
    State(pool): State<PgPool>,
    Path(project_id): Path<i64>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Project>> {
    let name = body["name"].as_str().ok_or(StatusCode::BAD_REQUEST)?;
    let project = sqlx::query_as::<_, Project>(
        "UPDATE projects SET name = $1 WHERE id = $2 \
         RETURNING id, repo_id, name, subdir, upstream",
    )
    .bind(name)
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(project))
}

pub async fn get_overview(State(pool): State<PgPool>) -> ApiResult<Json<OverviewJson>> {
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
        "SELECT short_sha FROM commits ORDER BY timestamp DESC LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(ise)?;

    Ok(Json(OverviewJson {
        observation_count,
        tracked_count,
        latest_commit_short_sha: latest.as_ref().map(|l| l.short_sha.clone()),
    }))
}

pub async fn get_project_overview(
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
        "SELECT c.short_sha FROM commits c \
         JOIN projects p ON p.repo_id = c.repo_id \
         WHERE p.id = $1 \
         ORDER BY c.timestamp DESC LIMIT 1",
    )
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?;

    Ok(Json(OverviewJson {
        observation_count,
        tracked_count,
        latest_commit_short_sha: latest.as_ref().map(|l| l.short_sha.clone()),
    }))
}

pub async fn get_users(State(pool): State<PgPool>) -> ApiResult<Json<Vec<String>>> {
    let rows: Vec<User> = sqlx::query_as("SELECT username FROM users ORDER BY username")
        .fetch_all(&pool)
        .await
        .map_err(ise)?;
    Ok(Json(rows.into_iter().map(|u| u.username).collect()))
}

// ── Benchmark PR ──────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct BenchmarkPrBody {
    #[serde(rename = "benchmarkName")]
    pub benchmark_name: String,
    pub files: std::collections::HashMap<String, String>,
}

pub async fn post_benchmark_pr(
    Path(project_id): Path<i64>,
    State(pool): State<PgPool>,
    Json(body): Json<BenchmarkPrBody>,
) -> ApiResult<Json<BenchmarkPrResponse>> {
    let project = sqlx::query_as::<_, Project>(
        "SELECT id, repo_id, name, subdir, upstream FROM projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let (owner, repo) = github_owner_repo(&project.upstream).ok_or(StatusCode::BAD_REQUEST)?;

    let token = std::env::var("GITHUB_TOKEN").map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
    let token = token.trim();
    if token.is_empty() {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }

    let client = Client::new();

    // Get repo default branch.
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

    // Get branch SHA.
    let ref_info: Value = github_api(
        &client,
        reqwest::Method::GET,
        &format!("https://api.github.com/repos/{owner}/{repo}/git/ref/heads/{default_branch}"),
        token,
        None,
    )
    .await?;
    let base_sha = ref_info["object"]["sha"]
        .as_str()
        .ok_or(StatusCode::BAD_GATEWAY)?
        .to_string();

    // Create blobs for each file.
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

    // Create tree.
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

    // Create commit.
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

    // Create branch.
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

    // Create PR.
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

    Ok(Json(BenchmarkPrResponse {
        pr_url: pr_url.to_string(),
    }))
}
