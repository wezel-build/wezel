use axum::http::StatusCode;
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::Client;
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{ApiResult, ise};

// ── Config ─────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct GithubAppConfig {
    pub app_id: i64,
    pub app_slug: String,
    pub client_id: String,
    pub client_secret: String,
    pub pem: String,
    pub webhook_secret: String,
    pub github_host: String,
}

pub type AppConfig = Arc<RwLock<Option<GithubAppConfig>>>;

pub fn new_app_config() -> AppConfig {
    Arc::new(RwLock::new(None))
}

// ── URL helpers ────────────────────────────────────────────────────────────

pub fn api_base_url(host: &str) -> String {
    if host == "github.com" {
        "https://api.github.com".to_string()
    } else {
        format!("https://{host}/api/v3")
    }
}

pub fn web_base_url(host: &str) -> String {
    format!("https://{host}")
}

// ── JWT ────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct JwtClaims {
    iat: u64,
    exp: u64,
    iss: String,
}

pub fn generate_jwt(app_id: i64, pem: &str) -> ApiResult<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(ise)?
        .as_secs();
    let claims = JwtClaims {
        iat: now.saturating_sub(60),
        exp: now + 600,
        iss: app_id.to_string(),
    };
    let key = EncodingKey::from_rsa_pem(pem.as_bytes()).map_err(ise)?;
    jsonwebtoken::encode(&Header::new(Algorithm::RS256), &claims, &key).map_err(ise)
}

// ── Installation tokens ────────────────────────────────────────────────────

pub async fn get_installation_token(
    pool: &PgPool,
    http: &Client,
    config: &GithubAppConfig,
    installation_id: i64,
) -> ApiResult<String> {
    // Check cache.
    let cached: Option<(String,)> = sqlx::query_as(
        "SELECT token FROM github_installation_tokens \
         WHERE installation_id = $1 AND expires_at > now() + interval '5 minutes'",
    )
    .bind(installation_id)
    .fetch_optional(pool)
    .await
    .map_err(ise)?;

    if let Some((token,)) = cached {
        return Ok(token);
    }

    let jwt = generate_jwt(config.app_id, &config.pem)?;
    let api_base = api_base_url(&config.github_host);
    let url = format!("{api_base}/app/installations/{installation_id}/access_tokens");

    let resp: Value = http
        .post(&url)
        .header("User-Agent", "wezel-burrow")
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(&jwt)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("failed to create installation token: {e}");
            StatusCode::BAD_GATEWAY
        })?
        .json()
        .await
        .map_err(|e| {
            tracing::error!("failed to parse installation token response: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    let token = resp["token"]
        .as_str()
        .ok_or(StatusCode::BAD_GATEWAY)?
        .to_string();
    let expires_at = resp["expires_at"]
        .as_str()
        .ok_or(StatusCode::BAD_GATEWAY)?;

    sqlx::query(
        "INSERT INTO github_installation_tokens (installation_id, token, expires_at) \
         VALUES ($1, $2, $3::timestamptz) \
         ON CONFLICT (installation_id) DO UPDATE SET token = $2, expires_at = $3::timestamptz",
    )
    .bind(installation_id)
    .bind(&token)
    .bind(expires_at)
    .execute(pool)
    .await
    .map_err(ise)?;

    Ok(token)
}

/// Resolve a GitHub API token for the given repo owner.
/// Returns None if no installation covers this owner.
pub async fn resolve_token(
    pool: &PgPool,
    http: &Client,
    config: &GithubAppConfig,
    owner: &str,
) -> ApiResult<Option<String>> {
    let installation: Option<(i64,)> = sqlx::query_as(
        "SELECT installation_id FROM github_app_installations \
         WHERE lower(account_login) = lower($1) AND suspended_at IS NULL",
    )
    .bind(owner)
    .fetch_optional(pool)
    .await
    .map_err(ise)?;

    let Some((installation_id,)) = installation else {
        return Ok(None);
    };

    let token = get_installation_token(pool, http, config, installation_id).await?;
    Ok(Some(token))
}

// ── Manifest ───────────────────────────────────────────────────────────────

pub fn build_manifest(app_name: &str, public_url: &str, github_host: &str) -> Value {
    let public_url = public_url.trim_end_matches('/');
    json!({
        "name": app_name,
        "url": public_url,
        "hook_attributes": {
            "url": format!("{public_url}/api/webhooks/github"),
            "active": true
        },
        "redirect_url": format!("{public_url}/api/setup/github-app/callback?github_host={github_host}"),
        "callback_urls": [format!("{public_url}/auth/github/callback")],
        "setup_url": format!("{public_url}/api/setup/github-app/install-callback"),
        "setup_on_update": true,
        "public": false,
        "default_permissions": {
            "contents": "write",
            "metadata": "read",
            "pull_requests": "write",
            "administration": "write"
        },
        "default_events": ["push"]
    })
}

// ── DB helpers ─────────────────────────────────────────────────────────────

pub async fn load_config(pool: &PgPool) -> sqlx::Result<Option<GithubAppConfig>> {
    let row: Option<(i64, String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT app_id, app_slug, client_id, client_secret, pem, webhook_secret, github_host \
         FROM github_app_config LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(
        |(app_id, app_slug, client_id, client_secret, pem, webhook_secret, github_host)| {
            GithubAppConfig {
                app_id,
                app_slug,
                client_id,
                client_secret,
                pem,
                webhook_secret,
                github_host,
            }
        },
    ))
}

pub async fn save_config(pool: &PgPool, config: &GithubAppConfig) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO github_app_config \
         (app_id, app_slug, client_id, client_secret, pem, webhook_secret, github_host) \
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(config.app_id)
    .bind(&config.app_slug)
    .bind(&config.client_id)
    .bind(&config.client_secret)
    .bind(&config.pem)
    .bind(&config.webhook_secret)
    .bind(&config.github_host)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn upsert_installation(
    pool: &PgPool,
    installation_id: i64,
    account_login: &str,
    account_type: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO github_app_installations (installation_id, account_login, account_type) \
         VALUES ($1, $2, $3) \
         ON CONFLICT (installation_id) DO UPDATE SET \
            account_login = $2, account_type = $3, updated_at = now(), suspended_at = NULL",
    )
    .bind(installation_id)
    .bind(account_login)
    .bind(account_type)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete_installation(pool: &PgPool, installation_id: i64) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM github_installation_tokens WHERE installation_id = $1")
        .bind(installation_id)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM github_app_installations WHERE installation_id = $1")
        .bind(installation_id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn suspend_installation(pool: &PgPool, installation_id: i64) -> sqlx::Result<()> {
    sqlx::query(
        "UPDATE github_app_installations SET suspended_at = now() WHERE installation_id = $1",
    )
    .bind(installation_id)
    .execute(pool)
    .await?;
    Ok(())
}
