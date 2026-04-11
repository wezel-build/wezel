use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
};
use reqwest::Client;
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::github::{github_api, github_owner_repo};
use crate::github_app;
use crate::models::*;
use crate::{AppState, ApiResult, ise};

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
pub async fn get_github_repos(
    State(state): State<AppState>,
) -> ApiResult<Json<Vec<Value>>> {
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
            let url = format!(
                "{api_base}/installation/repositories?per_page=100&page={page}"
            );
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

pub async fn setup_webhook(
    State(state): State<AppState>,
    Path(repo_id): Path<i64>,
) -> ApiResult<Json<WebhookSetupJson>> {
    let secret = format!(
        "whsec_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    );

    let row = sqlx::query_as::<_, Repo>(
        "UPDATE repos SET webhook_secret = $1 WHERE id = $2 \
         RETURNING id, upstream",
    )
    .bind(&secret)
    .bind(repo_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let webhook_url = burrow_webhook_url();
    let github_host = state.github_host();
    let auto_ok = if let Some((owner, repo)) = github_owner_repo(&row.upstream, &github_host) {
        match state.github_token(&owner).await {
            Ok(Some(token)) => {
                match register_github_webhook(&token, &state.api_base(), &owner, &repo, &webhook_url, &secret).await {
                    Ok(()) => true,
                    Err(e) => {
                        tracing::warn!(owner, repo, ?e, "webhook auto-registration failed");
                        false
                    }
                }
            }
            Ok(None) => {
                tracing::warn!(owner, repo, "no github app installation found for owner");
                false
            }
            Err(e) => {
                tracing::warn!(owner, repo, ?e, "failed to resolve github token");
                false
            }
        }
    } else {
        tracing::warn!(upstream = %row.upstream, github_host, "could not extract owner/repo from upstream");
        false
    };

    if auto_ok {
        sqlx::query("UPDATE repos SET webhook_registered = TRUE WHERE id = $1")
            .bind(repo_id)
            .execute(&state.pool)
            .await
            .map_err(ise)?;
    }

    Ok(Json(WebhookSetupJson {
        id: row.id,
        upstream: row.upstream,
        webhook_secret: secret,
        webhook_url,
        registered: auto_ok,
    }))
}

fn burrow_webhook_url() -> String {
    std::env::var("BURROW_PUBLIC_URL")
        .or_else(|_| std::env::var("FRONTEND_URL"))
        .unwrap_or_else(|_| "http://localhost:3001".to_string())
        .trim_end_matches('/')
        .to_string()
        + "/api/webhooks/github"
}

async fn register_github_webhook(
    token: &str,
    api_base: &str,
    owner: &str,
    repo: &str,
    webhook_url: &str,
    secret: &str,
) -> ApiResult<()> {
    let client = Client::new();

    let hooks: Vec<Value> = github_api(
        &client,
        reqwest::Method::GET,
        &format!("{api_base}/repos/{owner}/{repo}/hooks"),
        token,
        None,
    )
    .await?;

    for hook in &hooks {
        let url = hook
            .pointer("/config/url")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if url.contains("/api/webhooks/github")
            && let Some(id) = hook.get("id").and_then(|v| v.as_u64())
        {
            let del_client = Client::new();
            let _ = del_client
                .delete(format!("{api_base}/repos/{owner}/{repo}/hooks/{id}"))
                .header("User-Agent", "wezel-burrow")
                .header("Accept", "application/vnd.github+json")
                .bearer_auth(token)
                .send()
                .await;
        }
    }

    let _: Value = github_api(
        &client,
        reqwest::Method::POST,
        &format!("{api_base}/repos/{owner}/{repo}/hooks"),
        token,
        Some(serde_json::json!({
            "name": "web",
            "active": true,
            "events": ["push"],
            "config": {
                "url": webhook_url,
                "content_type": "json",
                "secret": secret,
                "insecure_ssl": "0"
            }
        })),
    )
    .await?;

    tracing::info!(owner, repo, "registered github webhook");
    Ok(())
}
