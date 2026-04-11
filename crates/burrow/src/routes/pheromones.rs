use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::Response,
};
use reqwest::Client;
use serde_json::Value;
use sqlx::PgPool;

use crate::models;
use crate::{AppState, ApiResult, cache_dir, ise};

pub fn pheromone_json_from_row(row: &models::PheromoneRow) -> models::PheromoneJson {
    let schema: Value = serde_json::from_str(&row.schema_json).unwrap_or(Value::Null);
    let platforms = schema
        .get("platforms")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let fields = schema
        .get("fields")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|f| models::PheromoneFieldJson {
                    name: f
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    field_type: f
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    description: f
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    deprecated: f
                        .get("deprecated")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                    deprecated_in: f
                        .get("deprecatedIn")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                    replaced_by: f
                        .get("replacedBy")
                        .and_then(|v| v.as_str())
                        .map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default();
    let viz_json = row
        .viz_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());
    models::PheromoneJson {
        id: row.id,
        name: row.name.clone(),
        github_repo: row.github_repo.clone(),
        version: row.version.clone(),
        platforms,
        fields,
        fetched_at: row.fetched_at.clone(),
        viz_json,
    }
}

pub fn get_dev_dir() -> Option<std::path::PathBuf> {
    std::env::var("WEZEL_PHEROMONE_DEV_DIR")
        .ok()
        .map(Into::into)
}

async fn fetch_and_store_from_local(
    pool: &PgPool,
    dev_dir: &std::path::Path,
    name: &str,
    github_repo: &str,
) -> ApiResult<models::PheromoneJson> {
    let schema: Value = {
        let s = std::fs::read_to_string(dev_dir.join(name).join("schema.json")).map_err(ise)?;
        serde_json::from_str(&s).map_err(ise)?
    };
    let pheromone_name = schema
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or(name)
        .to_string();
    let version = schema
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();
    let schema_str = serde_json::to_string(&schema).map_err(ise)?;

    let viz_str: Option<String> = std::fs::read_to_string(dev_dir.join(name).join("viz.json"))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| serde_json::to_string(&v).ok());

    let row = sqlx::query_as::<_, models::PheromoneRow>(
        "INSERT INTO pheromones (name, github_repo, version, schema_json, viz_json)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (name) DO UPDATE SET github_repo = $2, version = $3, schema_json = $4, viz_json = $5, fetched_at = now()
         RETURNING id, name, github_repo, version, schema_json, viz_json, fetched_at::TEXT as fetched_at",
    )
    .bind(&pheromone_name)
    .bind(github_repo)
    .bind(&version)
    .bind(&schema_str)
    .bind(&viz_str)
    .fetch_one(pool)
    .await
    .map_err(ise)?;

    let _ = sqlx::query(
        "INSERT INTO pheromone_schema_history (pheromone_id, version, schema_json)
         VALUES ($1, $2, $3)
         ON CONFLICT (pheromone_id, version) DO NOTHING",
    )
    .bind(row.id)
    .bind(&version)
    .bind(&schema_str)
    .execute(pool)
    .await;

    tracing::info!("loaded dev pheromone: {pheromone_name}");
    Ok(pheromone_json_from_row(&row))
}

pub async fn load_dev_pheromones(state: &AppState, dev_dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dev_dir) else {
        tracing::warn!(
            "WEZEL_PHEROMONE_DEV_DIR not readable: {}",
            dev_dir.display()
        );
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if !path.join("schema.json").exists() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let github_repo =
            sqlx::query_scalar::<_, String>("SELECT github_repo FROM pheromones WHERE name = $1")
                .bind(name.as_ref())
                .fetch_optional(&state.pool)
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| format!("dev/{name}"));

        if let Err(e) = fetch_and_store_pheromone(state, &github_repo).await {
            tracing::warn!("failed to load dev pheromone {name}: {e:?}");
        }
    }
}

/// Resolve an optional GitHub token for the given `owner/repo` style github_repo string.
async fn resolve_pheromone_token(state: &AppState, github_repo: &str) -> Option<String> {
    let owner = github_repo.split('/').next()?;
    state.github_token(owner).await.ok().flatten()
}

fn github_request(
    client: &Client,
    url: &str,
    token: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut req = client.get(url).header("User-Agent", "wezel-burrow");
    if let Some(token) = token {
        req = req.bearer_auth(token);
    }
    req
}

pub async fn fetch_and_store_pheromone(
    state: &AppState,
    github_repo: &str,
) -> ApiResult<models::PheromoneJson> {
    let name = github_repo.split('/').next_back().unwrap_or(github_repo);
    if let Some(dev_dir) = get_dev_dir()
        && dev_dir.join(name).join("schema.json").exists()
    {
        return fetch_and_store_from_local(&state.pool, &dev_dir, name, github_repo).await;
    }

    let token = resolve_pheromone_token(state, github_repo).await;
    let api_base = state.api_base();

    let client = Client::new();
    let url = format!("{api_base}/repos/{github_repo}/releases/latest");
    let resp = github_request(&client, &url, token.as_deref())
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    if !resp.status().is_success() {
        return Err(StatusCode::BAD_GATEWAY);
    }
    let release: Value = resp.json().await.map_err(|_| StatusCode::BAD_GATEWAY)?;

    let assets = release
        .get("assets")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_GATEWAY)?;
    let schema_url = assets
        .iter()
        .find(|a| a.get("name").and_then(|v| v.as_str()) == Some("schema.json"))
        .and_then(|a| a.get("browser_download_url"))
        .and_then(|v| v.as_str())
        .ok_or(StatusCode::NOT_FOUND)?
        .to_string();

    let schema_resp = github_request(&client, &schema_url, token.as_deref())
        .send()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    let schema: Value = schema_resp
        .json()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;

    let name = schema
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| github_repo.split('/').next_back().unwrap_or(github_repo))
        .to_string();
    let version = schema
        .get("version")
        .and_then(|v| v.as_str())
        .unwrap_or("0.0.0")
        .to_string();
    let schema_str = serde_json::to_string(&schema).map_err(ise)?;

    let viz_str: Option<String> = if let Some(viz_url) = assets
        .iter()
        .find(|a| a.get("name").and_then(|v| v.as_str()) == Some("viz.json"))
        .and_then(|a| a.get("browser_download_url"))
        .and_then(|v| v.as_str())
    {
        if let Ok(resp) = github_request(&client, viz_url, token.as_deref())
            .send()
            .await
        {
            if let Ok(val) = resp.json::<Value>().await {
                serde_json::to_string(&val).ok()
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let row = sqlx::query_as::<_, models::PheromoneRow>(
        "INSERT INTO pheromones (name, github_repo, version, schema_json, viz_json)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (name) DO UPDATE SET github_repo = $2, version = $3, schema_json = $4, viz_json = $5, fetched_at = now()
         RETURNING id, name, github_repo, version, schema_json, viz_json, fetched_at::TEXT as fetched_at",
    )
    .bind(&name)
    .bind(github_repo)
    .bind(&version)
    .bind(&schema_str)
    .bind(&viz_str)
    .fetch_one(&state.pool)
    .await
    .map_err(ise)?;

    let _ = sqlx::query(
        "INSERT INTO pheromone_schema_history (pheromone_id, version, schema_json)
         VALUES ($1, $2, $3)
         ON CONFLICT (pheromone_id, version) DO NOTHING",
    )
    .bind(row.id)
    .bind(&version)
    .bind(&schema_str)
    .execute(&state.pool)
    .await;

    Ok(pheromone_json_from_row(&row))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
pub struct AdminPheromoneBody {
    github_repo: String,
}

pub async fn get_pheromones(
    State(pool): State<PgPool>,
) -> ApiResult<Json<Vec<models::PheromoneJson>>> {
    let rows = sqlx::query_as::<_, models::PheromoneRow>(
        "SELECT id, name, github_repo, version, schema_json, viz_json, fetched_at::TEXT as fetched_at
         FROM pheromones ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .map_err(ise)?;
    Ok(Json(rows.iter().map(pheromone_json_from_row).collect()))
}

pub async fn get_admin_pheromones(
    State(pool): State<PgPool>,
) -> ApiResult<Json<Vec<models::PheromoneJson>>> {
    get_pheromones(State(pool)).await
}

pub async fn post_admin_pheromone(
    State(state): State<AppState>,
    Json(body): Json<AdminPheromoneBody>,
) -> ApiResult<(StatusCode, Json<models::PheromoneJson>)> {
    let pheromone = fetch_and_store_pheromone(&state, &body.github_repo).await?;
    Ok((StatusCode::CREATED, Json(pheromone)))
}

pub async fn post_admin_pheromone_fetch(
    Path(name): Path<String>,
    State(state): State<AppState>,
) -> ApiResult<Json<models::PheromoneJson>> {
    let row = sqlx::query_as::<_, models::PheromoneRow>(
        "SELECT id, name, github_repo, version, schema_json, fetched_at::TEXT as fetched_at
         FROM pheromones WHERE name = $1",
    )
    .bind(&name)
    .fetch_optional(&state.pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;
    let pheromone = fetch_and_store_pheromone(&state, &row.github_repo).await?;
    Ok(Json(pheromone))
}

pub async fn get_pheromone_binary(
    Path((name, target)): Path<(String, String)>,
    State(state): State<AppState>,
) -> Result<Response, StatusCode> {
    let row = sqlx::query_as::<_, models::PheromoneRow>(
        "SELECT id, name, github_repo, version, schema_json, viz_json, fetched_at::TEXT as fetched_at
         FROM pheromones WHERE name = $1",
    )
    .bind(&name)
    .fetch_optional(&state.pool)
    .await
    .map_err(ise)?
    .ok_or(StatusCode::NOT_FOUND)?;

    let tarball_name = format!("{name}-{target}.tar.gz");

    // Dev mode: serve directly from local dev dir, no caching.
    if let Some(dev_dir) = get_dev_dir() {
        let path = dev_dir.join(&name).join(&tarball_name);
        if path.exists() {
            let bytes = std::fs::read(&path).map_err(ise)?;
            return Response::builder()
                .header("content-type", "application/octet-stream")
                .body(axum::body::Body::from(bytes))
                .map_err(ise);
        }
    }

    let cache_path = cache_dir()
        .join(&name)
        .join(&row.version)
        .join(&tarball_name);

    if !cache_path.exists() {
        let github_host = state.github_host();
        let download_url = format!(
            "https://{github_host}/{}/releases/download/v{}/{}",
            row.github_repo, row.version, tarball_name
        );
        let token = resolve_pheromone_token(&state, &row.github_repo).await;
        let client = Client::new();
        let resp = github_request(&client, &download_url, token.as_deref())
            .send()
            .await
            .map_err(|_| StatusCode::BAD_GATEWAY)?;
        if !resp.status().is_success() {
            return Err(StatusCode::BAD_GATEWAY);
        }
        let bytes = resp.bytes().await.map_err(|_| StatusCode::BAD_GATEWAY)?;
        if let Some(parent) = cache_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(ise)?;
        }
        tokio::fs::write(&cache_path, &bytes).await.map_err(ise)?;
    }

    let bytes = tokio::fs::read(&cache_path).await.map_err(ise)?;
    Response::builder()
        .header("content-type", "application/octet-stream")
        .body(axum::body::Body::from(bytes))
        .map_err(ise)
}
