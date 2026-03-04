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

#[derive(Clone)]
pub struct AuthUser {
    pub login: String,
}

pub async fn login() -> impl IntoResponse {
    let state = Uuid::new_v4().to_string();
    let client_id = std::env::var("GITHUB_CLIENT_ID").expect("GITHUB_CLIENT_ID not set");
    let url = format!(
        "https://github.com/login/oauth/authorize?client_id={client_id}&state={state}&scope=read:user"
    );
    let state_cookie = Cookie::build(("oauth_state", state))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .build();
    (CookieJar::new().add(state_cookie), Redirect::to(&url))
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    pub code: String,
    pub state: String,
}

pub async fn callback(
    State(pool): State<PgPool>,
    Query(q): Query<CallbackQuery>,
    jar: CookieJar,
) -> Result<impl IntoResponse, StatusCode> {
    // Verify CSRF state
    let expected = jar.get("oauth_state").map(|c| c.value().to_string());
    if expected.as_deref() != Some(q.state.as_str()) {
        return Err(StatusCode::BAD_REQUEST);
    }

    let client_id =
        std::env::var("GITHUB_CLIENT_ID").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let client_secret =
        std::env::var("GITHUB_CLIENT_SECRET").map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let http = reqwest::Client::new();

    // Exchange code for access token
    let token_res: serde_json::Value = http
        .post("https://github.com/login/oauth/access_token")
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

    // Fetch GitHub user info
    let user_res: serde_json::Value = http
        .get("https://api.github.com/user")
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

    // Persist session
    let session_id = Uuid::new_v4().to_string();
    db::create_session(&pool, &session_id, &login)
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

pub async fn logout(State(pool): State<PgPool>, jar: CookieJar) -> impl IntoResponse {
    if let Some(c) = jar.get("session_id") {
        let _ = db::delete_session(&pool, c.value()).await;
    }
    let jar = jar.remove(Cookie::from("session_id"));
    (jar, StatusCode::NO_CONTENT)
}
