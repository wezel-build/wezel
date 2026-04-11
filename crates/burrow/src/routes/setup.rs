use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::github_app::{self, GithubAppConfig};
use crate::{ApiResult, AppState, ise};

/// GET /api/setup/status
pub async fn get_setup_status(State(state): State<AppState>) -> Json<Value> {
    let config = state.github_app.read().unwrap();
    match config.as_ref() {
        Some(c) => Json(json!({
            "configured": true,
            "github_host": c.github_host,
            "app_slug": c.app_slug,
        })),
        None => Json(json!({
            "configured": false,
        })),
    }
}

#[derive(Deserialize)]
pub struct ManifestBody {
    github_host: Option<String>,
    public_url: String,
    app_name: Option<String>,
}

/// POST /api/setup/github-app/manifest
pub async fn post_manifest(
    State(state): State<AppState>,
    Json(body): Json<ManifestBody>,
) -> ApiResult<Json<Value>> {
    let config = state.github_app.read().map_err(ise)?;
    if config.is_some() {
        return Err(StatusCode::CONFLICT);
    }

    let github_host = body.github_host.as_deref().unwrap_or("github.com");
    let default_name = format!(
        "Wezel ({})",
        body.public_url
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
    );
    let app_name = body.app_name.as_deref().unwrap_or(&default_name);
    let manifest = github_app::build_manifest(app_name, &body.public_url, github_host);
    let post_url = format!(
        "{}/settings/apps/new",
        github_app::web_base_url(github_host)
    );

    Ok(Json(json!({
        "manifest": manifest,
        "post_url": post_url,
        "github_host": github_host,
    })))
}

#[derive(Deserialize)]
pub struct AppCallbackQuery {
    code: String,
    #[serde(default)]
    github_host: Option<String>,
}

/// GET /api/setup/github-app/callback?code=XXX&github_host=...
pub async fn get_app_callback(
    State(state): State<AppState>,
    Query(q): Query<AppCallbackQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    {
        let config = state.github_app.read().map_err(ise)?;
        if config.is_some() {
            return Err(StatusCode::CONFLICT);
        }
    }

    let github_host = q.github_host.as_deref().unwrap_or("github.com");
    let api_base = github_app::api_base_url(github_host);
    let url = format!("{api_base}/app-manifests/{}/conversions", q.code);

    let resp: Value = state
        .http
        .post(&url)
        .header("User-Agent", "wezel-burrow")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            tracing::error!("manifest exchange failed: {e}");
            StatusCode::BAD_GATEWAY
        })?
        .json()
        .await
        .map_err(|e| {
            tracing::error!("manifest exchange parse failed: {e}");
            StatusCode::BAD_GATEWAY
        })?;

    let config = GithubAppConfig {
        app_id: resp["id"].as_i64().ok_or(StatusCode::BAD_GATEWAY)?,
        app_slug: resp["slug"]
            .as_str()
            .ok_or(StatusCode::BAD_GATEWAY)?
            .to_string(),
        client_id: resp["client_id"]
            .as_str()
            .ok_or(StatusCode::BAD_GATEWAY)?
            .to_string(),
        client_secret: resp["client_secret"]
            .as_str()
            .ok_or(StatusCode::BAD_GATEWAY)?
            .to_string(),
        pem: resp["pem"]
            .as_str()
            .ok_or(StatusCode::BAD_GATEWAY)?
            .to_string(),
        webhook_secret: resp["webhook_secret"]
            .as_str()
            .ok_or(StatusCode::BAD_GATEWAY)?
            .to_string(),
        github_host: github_host.to_string(),
    };

    github_app::save_config(&state.pool, &config)
        .await
        .map_err(ise)?;

    let web_base = github_app::web_base_url(github_host);
    let install_url = format!("{web_base}/apps/{}/installations/new", config.app_slug);

    // Update in-memory config.
    {
        let mut guard = state.github_app.write().map_err(ise)?;
        *guard = Some(config);
    }

    Ok(Redirect::to(&install_url))
}

#[derive(Deserialize)]
pub struct InstallCallbackQuery {
    installation_id: Option<i64>,
    #[allow(unused)]
    setup_action: Option<String>,
}

/// GET /api/setup/github-app/install-callback
pub async fn get_install_callback(
    State(state): State<AppState>,
    Query(q): Query<InstallCallbackQuery>,
) -> Result<impl IntoResponse, StatusCode> {
    let (app_id, pem, github_host) = {
        let config_guard = state.github_app.read().map_err(ise)?;
        let config = config_guard
            .as_ref()
            .ok_or(StatusCode::PRECONDITION_FAILED)?;
        (
            config.app_id,
            config.pem.clone(),
            config.github_host.clone(),
        )
    };

    if let Some(installation_id) = q.installation_id {
        let jwt = github_app::generate_jwt(app_id, &pem)?;
        let api_base = github_app::api_base_url(&github_host);
        let url = format!("{api_base}/app/installations/{installation_id}");

        let http_resp = state
            .http
            .get(&url)
            .header("User-Agent", "wezel-burrow")
            .header("Accept", "application/vnd.github+json")
            .bearer_auth(&jwt)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("installation fetch failed: {e}");
                StatusCode::BAD_GATEWAY
            })?;

        let (account_login, account_type) = if http_resp.status().is_success() {
            let resp: Value = http_resp.json().await.map_err(|e| {
                tracing::error!("installation fetch parse failed: {e}");
                StatusCode::BAD_GATEWAY
            })?;
            tracing::info!(installation_id, %resp, "installation details from GitHub");
            (
                resp["account"]["login"]
                    .as_str()
                    .unwrap_or("unknown")
                    .to_string(),
                resp["account"]["type"]
                    .as_str()
                    .unwrap_or("Organization")
                    .to_string(),
            )
        } else {
            let status = http_resp.status();
            let body = http_resp.text().await.unwrap_or_default();
            tracing::warn!(
                installation_id,
                %status,
                body,
                "failed to fetch installation details from GitHub, using installation_id as fallback"
            );
            // Save with installation_id so token resolution can still work —
            // the account_login will be wrong but the installation is tracked.
            (
                format!("installation-{installation_id}"),
                "Organization".to_string(),
            )
        };

        github_app::upsert_installation(
            &state.pool,
            installation_id,
            &account_login,
            &account_type,
        )
        .await
        .map_err(ise)?;

        tracing::info!(installation_id, account_login, "github app installed");
    }

    let frontend_url =
        std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());

    Ok(Redirect::to(&frontend_url))
}
