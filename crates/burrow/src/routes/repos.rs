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
use crate::models::*;
use crate::{ApiResult, ise};

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

pub async fn setup_webhook(
    State(pool): State<PgPool>,
    Path(repo_id): Path<i64>,
) -> ApiResult<Json<WebhookSetupJson>> {
    // Generate secret.
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
    .fetch_optional(&pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;

    // Try to register on GitHub automatically.
    let webhook_url = burrow_webhook_url();
    let auto_ok = if let Some(token) = github_token() {
        if let Some((owner, repo)) = github_owner_repo(&row.upstream) {
            register_github_webhook(&token, &owner, &repo, &webhook_url, &secret)
                .await
                .is_ok()
        } else {
            false
        }
    } else {
        false
    };

    // Only mark as registered if GitHub confirmed.
    if auto_ok {
        sqlx::query("UPDATE repos SET webhook_registered = TRUE WHERE id = $1")
            .bind(repo_id)
            .execute(&pool)
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

fn github_token() -> Option<String> {
    std::env::var("GITHUB_TOKEN")
        .ok()
        .filter(|t| !t.trim().is_empty())
        .map(|t| t.trim().to_string())
}

fn burrow_webhook_url() -> String {
    std::env::var("BURROW_PUBLIC_URL")
        .unwrap_or_else(|_| "http://localhost:3001".to_string())
        .trim_end_matches('/')
        .to_string()
        + "/api/webhooks/github"
}

async fn register_github_webhook(
    token: &str,
    owner: &str,
    repo: &str,
    webhook_url: &str,
    secret: &str,
) -> ApiResult<()> {
    let client = Client::new();

    // Check for existing wezel webhook and delete it.
    let hooks: Vec<Value> = github_api(
        &client,
        reqwest::Method::GET,
        &format!("https://api.github.com/repos/{owner}/{repo}/hooks"),
        token,
        None,
    )
    .await?;

    for hook in &hooks {
        let url = hook
            .pointer("/config/url")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if url.contains("/api/webhooks/github") {
            if let Some(id) = hook.get("id").and_then(|v| v.as_u64()) {
                let del_client = Client::new();
                let _ = del_client
                    .delete(format!(
                        "https://api.github.com/repos/{owner}/{repo}/hooks/{id}"
                    ))
                    .header("User-Agent", "wezel-burrow")
                    .header("Accept", "application/vnd.github+json")
                    .bearer_auth(token)
                    .send()
                    .await;
            }
        }
    }

    // Create new webhook.
    let _: Value = github_api(
        &client,
        reqwest::Method::POST,
        &format!("https://api.github.com/repos/{owner}/{repo}/hooks"),
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
