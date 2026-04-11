use axum::{Json, extract::State, http::StatusCode};
use serde_json::Value;
use sqlx::PgPool;

use crate::github_app;
use crate::models::*;
use crate::{ApiResult, AppState, ise};

pub async fn get_repos(State(pool): State<PgPool>) -> ApiResult<Json<Vec<RepoJson>>> {
    let rows = sqlx::query_as::<_, RepoRow>(
        "SELECT r.id, r.upstream, r.webhook_registered, \
                COUNT(p.id) AS project_count \
         FROM repos r \
         LEFT JOIN projects p ON p.repo_id = r.id \
         GROUP BY r.id \
         ORDER BY r.id",
    )
    .fetch_all(&pool)
    .await
    .map_err(ise)?;

    Ok(Json(
        rows.into_iter()
            .map(|r| RepoJson {
                id: r.id,
                upstream: r.upstream,
                webhook_registered: r.webhook_registered,
                project_count: r.project_count,
            })
            .collect(),
    ))
}

/// GET /api/github/repos — list repos accessible via GitHub App installations.
pub async fn get_github_repos(State(state): State<AppState>) -> ApiResult<Json<Vec<Value>>> {
    let config = {
        let guard = state.github_app.read().map_err(ise)?;
        guard.clone()
    };
    let Some(config) = config.as_ref() else {
        return Ok(Json(vec![]));
    };

    let installations: Vec<(i64, String)> = sqlx::query_as(
        "SELECT installation_id, account_login FROM github_app_installations WHERE suspended_at IS NULL",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(ise)?;

    let api_base = github_app::api_base_url(&config.github_host);
    let mut all_repos = Vec::new();

    for (installation_id, _account) in &installations {
        let token =
            github_app::get_installation_token(&state.pool, &state.http, config, *installation_id)
                .await?;

        // Paginate through installation repos.
        let mut page = 1u32;
        loop {
            let url = format!("{api_base}/installation/repositories?per_page=100&page={page}");
            let resp: Value = state
                .http
                .get(&url)
                .header("User-Agent", "wezel-burrow")
                .header("Accept", "application/vnd.github+json")
                .bearer_auth(&token)
                .send()
                .await
                .map_err(|e| {
                    tracing::error!("failed to list installation repos: {e}");
                    StatusCode::BAD_GATEWAY
                })?
                .json()
                .await
                .map_err(|e| {
                    tracing::error!("failed to parse installation repos: {e}");
                    StatusCode::BAD_GATEWAY
                })?;

            let repos = resp["repositories"].as_array();
            let Some(repos) = repos else { break };
            if repos.is_empty() {
                break;
            }

            for repo in repos {
                let full_name = repo["full_name"].as_str().unwrap_or("").to_string();
                let html_url = repo["html_url"].as_str().unwrap_or("").to_string();
                let private = repo["private"].as_bool().unwrap_or(false);
                all_repos.push(serde_json::json!({
                    "full_name": full_name,
                    "html_url": html_url,
                    "private": private,
                }));
            }

            let total = resp["total_count"].as_u64().unwrap_or(0);
            if all_repos.len() as u64 >= total {
                break;
            }
            page += 1;
        }
    }

    Ok(Json(all_repos))
}
