use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use serde::Deserialize;
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db;
use crate::github_app;
use crate::{AppState, ise};

#[derive(Clone)]
pub struct AuthUser {
    #[expect(unused)]
    pub login: String,
}

pub async fn login(State(state): State<AppState>) -> Result<impl IntoResponse, StatusCode> {
    let config_guard = state.github_app.read().map_err(ise)?;
    let config = config_guard.as_ref().ok_or(StatusCode::NOT_IMPLEMENTED)?;

    let csrf_state = Uuid::new_v4().to_string();
    let web_base = github_app::web_base_url(&config.github_host);
    let client_id = &config.client_id;
    let url = format!(
        "{web_base}/login/oauth/authorize?client_id={client_id}&state={csrf_state}&scope=read:user"
    );
    let state_cookie = Cookie::build(("oauth_state", csrf_state))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .build();
    Ok((CookieJar::new().add(state_cookie), Redirect::to(&url)))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

pub async fn callback(
    State(state): State<AppState>,
    Query(q): Query<CallbackQuery>,
    jar: CookieJar,
) -> Result<impl IntoResponse, StatusCode> {
    let expected = jar.get("oauth_state").map(|c| c.value().to_string());
    if expected.as_deref() != Some(q.state.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let (web_base, api_base, client_id, client_secret) = {
        let config_guard = state.github_app.read().map_err(ise)?;
        let config = config_guard
            .as_ref()
            .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
        (
            github_app::web_base_url(&config.github_host),
            github_app::api_base_url(&config.github_host),
            config.client_id.clone(),
            config.client_secret.clone(),
        )
    };

    // Exchange code for access token.
    let token_res: serde_json::Value = state
        .http
        .post(format!("{web_base}/login/oauth/access_token"))
        .header("Accept", "application/json")
        .form(&[
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("code", q.code.as_str()),
        ])
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?
        .json()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let access_token = token_res["access_token"]
        .as_str()
        .ok_or(StatusCode::BAD_GATEWAY)?
        .to_string();

    // Fetch GitHub user info.
    let user_res: serde_json::Value = state
        .http
        .get(format!("{api_base}/user"))
        .header("Authorization", format!("Bearer {access_token}"))
        .header("User-Agent", "wezel")
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?
        .json()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let login = user_res["login"]
        .as_str()
        .ok_or(StatusCode::BAD_GATEWAY)?
        .to_string();

    // Persist session.
    let session_id = Uuid::new_v4().to_string();
    db::create_session(&state.pool, &session_id, &login)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let frontend_url =
        std::env::var("FRONTEND_URL").unwrap_or_else(|_| "http://localhost:5173".to_string());

    let session_cookie = Cookie::build(("session_id", session_id))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .build();

    let jar = jar.remove(Cookie::from("oauth_state")).add(session_cookie);

    Ok((jar, Redirect::to(&frontend_url)))
}

pub async fn me(
    State(pool): State<PgPool>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let session_id = jar
        .get("session_id")
        .map(|c| c.value().to_string())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    match db::get_session(&pool, &session_id).await {
        Ok(Some(login)) => Ok(Json(json!({ "login": login }))),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

pub async fn config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.github_app.read().ok();
    let configured = config.as_ref().map(|c| c.is_some()).unwrap_or(false);

    if configured {
        let c = config.unwrap();
        let c = c.as_ref().unwrap();
        Json(json!({
            "auth_required": true,
            "setup_required": false,
            "github_host": c.github_host,
            "app_slug": c.app_slug,
        }))
    } else {
        Json(json!({
            "auth_required": false,
            "setup_required": true,
        }))
    }
}

pub async fn logout(State(pool): State<PgPool>, jar: CookieJar) -> impl IntoResponse {
    if let Some(c) = jar.get("session_id") {
        let _ = db::delete_session(&pool, c.value()).await;
    }
    let jar = jar.remove(Cookie::from("session_id"));
    (jar, StatusCode::NO_CONTENT)
}
