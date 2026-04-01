use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use axum::response::Response;
use serde::Serialize;

use crate::{ApiResult, cache_dir, ise};

// ── In-memory release state ──────────────────────────────────────────────────

#[derive(Clone)]
struct ReleaseAsset {
    filename: String,
    download_url: String,
}

/// A single tool derived from the dist-manifest.json.
#[derive(Clone)]
struct Tool {
    /// Binary name (e.g. "forager-exec").
    name: String,
    /// Version string.
    version: String,
    /// target → archive asset.
    assets: HashMap<String, ReleaseAsset>,
}

#[derive(Clone)]
pub struct ToolRelease {
    /// Internal: the git tag.
    tag: String,
    /// binary name → Tool.
    tools: HashMap<String, Tool>,
}

static TOOL_RELEASE: OnceLock<RwLock<Option<Arc<ToolRelease>>>> = OnceLock::new();

fn release_store() -> &'static RwLock<Option<Arc<ToolRelease>>> {
    TOOL_RELEASE.get_or_init(|| RwLock::new(None))
}

fn get_release() -> Option<Arc<ToolRelease>> {
    release_store().read().ok()?.clone()
}

fn set_release(release: ToolRelease) {
    if let Ok(mut guard) = release_store().write() {
        *guard = Some(Arc::new(release));
    }
}

fn tool_repo() -> String {
    std::env::var("WEZEL_TOOL_REPO").unwrap_or_else(|_| "wezel-build/wezel".to_string())
}

fn github_request(client: &reqwest::Client, url: &str) -> reqwest::RequestBuilder {
    let mut req = client.get(url).header("User-Agent", "wezel-burrow");
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim().to_string();
        if !token.is_empty() {
            req = req.bearer_auth(token);
        }
    }
    req
}

// ── GitHub release fetching ──────────────────────────────────────────────────

/// Fetch the latest nightly release from GitHub and update in-memory state.
/// Short-circuits if the tag hasn't changed.
pub async fn refresh_tool_release() -> Result<(), StatusCode> {
    let client = reqwest::Client::new();
    let repo = tool_repo();

    // Find the latest nightly release.
    let releases_url = format!("https://api.github.com/repos/{repo}/releases");
    let resp = github_request(&client, &releases_url)
        .query(&[("per_page", "10")])
        .send()
        .await
        .map_err(|e| {
            tracing::warn!("failed to fetch releases: {e}");
            StatusCode::BAD_GATEWAY
        })?;
    if !resp.status().is_success() {
        tracing::warn!("GitHub releases API returned {}", resp.status());
        return Err(StatusCode::BAD_GATEWAY);
    }

    let releases: Vec<serde_json::Value> = resp.json().await.map_err(|e| {
        tracing::warn!("failed to parse releases JSON: {e}");
        StatusCode::BAD_GATEWAY
    })?;

    let release = releases
        .iter()
        .find(|r| {
            r.get("tag_name")
                .and_then(|v| v.as_str())
                .is_some_and(|t| t.starts_with("nightly-"))
        })
        .ok_or_else(|| {
            tracing::debug!("no nightly release found in {repo}");
            StatusCode::NOT_FOUND
        })?;

    let tag = release["tag_name"].as_str().unwrap().to_string();

    if let Some(current) = get_release()
        && current.tag == tag
    {
        return Ok(());
    }

    // Build a lookup of GitHub release assets: filename → download URL.
    let gh_assets = release["assets"]
        .as_array()
        .ok_or(StatusCode::BAD_GATEWAY)?;
    let gh_asset_map: HashMap<String, String> = gh_assets
        .iter()
        .filter_map(|a| {
            let name = a["name"].as_str()?.to_string();
            let url = a["browser_download_url"].as_str()?.to_string();
            Some((name, url))
        })
        .collect();

    // Fetch dist-manifest.json from the release.
    let manifest_url = gh_asset_map
        .get("dist-manifest.json")
        .ok_or_else(|| {
            tracing::warn!("nightly release {tag} has no dist-manifest.json");
            StatusCode::NOT_FOUND
        })?;

    let manifest_resp = github_request(&client, manifest_url)
        .send()
        .await
        .map_err(|e| {
            tracing::warn!("failed to fetch dist-manifest.json: {e}");
            StatusCode::BAD_GATEWAY
        })?;
    let manifest: serde_json::Value = manifest_resp.json().await.map_err(|e| {
        tracing::warn!("failed to parse dist-manifest.json: {e}");
        StatusCode::BAD_GATEWAY
    })?;

    // Parse the manifest: artifacts map has archive entries with executable assets.
    let artifacts = manifest
        .get("artifacts")
        .and_then(|v| v.as_object())
        .ok_or(StatusCode::BAD_GATEWAY)?;

    // Also parse releases for package → version mapping.
    let manifest_releases = manifest
        .get("releases")
        .and_then(|v| v.as_array())
        .ok_or(StatusCode::BAD_GATEWAY)?;
    let mut package_versions: HashMap<String, String> = HashMap::new();
    for r in manifest_releases {
        if let (Some(name), Some(ver)) = (
            r.get("app_name").and_then(|v| v.as_str()),
            r.get("app_version").and_then(|v| v.as_str()),
        ) {
            package_versions.insert(name.to_string(), ver.to_string());
        }
    }

    let mut tools: HashMap<String, Tool> = HashMap::new();

    for (archive_name, artifact) in artifacts {
        if artifact.get("kind").and_then(|v| v.as_str()) != Some("executable-zip") {
            continue;
        }

        let targets = artifact
            .get("target_triples")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        // Find the executable binary name inside the archive.
        let binary_name = artifact
            .get("assets")
            .and_then(|v| v.as_array())
            .and_then(|assets| {
                assets.iter().find_map(|a| {
                    if a.get("kind").and_then(|v| v.as_str()) == Some("executable") {
                        a.get("name").and_then(|v| v.as_str()).map(String::from)
                    } else {
                        None
                    }
                })
            });

        let Some(binary_name) = binary_name else {
            continue;
        };

        let Some(download_url) = gh_asset_map.get(archive_name.as_str()) else {
            continue;
        };

        // Derive package name from archive: "{package}-{version}-{target}.tar.xz"
        let package = archive_name
            .split('-')
            .next()
            .unwrap_or(archive_name)
            .to_string();

        let version = package_versions
            .get(&package)
            .cloned()
            .unwrap_or_default();

        let tool = tools.entry(binary_name.clone()).or_insert_with(|| Tool {
            name: binary_name.clone(),
            version: version.clone(),
            assets: HashMap::new(),
        });

        for target in &targets {
            tool.assets.entry(target.clone()).or_insert(ReleaseAsset {
                filename: archive_name.clone(),
                download_url: download_url.clone(),
            });
        }
    }

    if tools.is_empty() {
        tracing::warn!("nightly release {tag}: dist-manifest.json has no tools");
        return Err(StatusCode::NOT_FOUND);
    }

    tracing::info!(
        "refreshed tool release: {tag} ({} tools, {} total archives)",
        tools.len(),
        tools.values().map(|t| t.assets.len()).sum::<usize>(),
    );
    set_release(ToolRelease { tag, tools });
    Ok(())
}

// ── API types ────────────────────────────────────────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolJson {
    pub name: String,
    pub version: String,
    pub targets: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsResponse {
    pub tools: Vec<ToolJson>,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn ensure_release() -> Result<Arc<ToolRelease>, StatusCode> {
    if let Some(r) = get_release() {
        return Ok(r);
    }
    refresh_tool_release().await?;
    get_release().ok_or(StatusCode::SERVICE_UNAVAILABLE)
}

/// GET /api/tools
pub async fn get_tools() -> ApiResult<Json<ToolsResponse>> {
    let release = ensure_release().await?;

    let mut tools: Vec<ToolJson> = release
        .tools
        .values()
        .map(|tool| {
            let mut targets: Vec<String> = tool.assets.keys().cloned().collect();
            targets.sort();
            ToolJson {
                name: tool.name.clone(),
                version: tool.version.clone(),
                targets,
            }
        })
        .collect();
    tools.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(Json(ToolsResponse { tools }))
}

/// GET /api/tools/{name}/binary/{target}
pub async fn get_tool_binary(
    Path((name, target)): Path<(String, String)>,
) -> Result<Response, StatusCode> {
    let release = ensure_release().await?;

    let tool = release.tools.get(&name).ok_or(StatusCode::NOT_FOUND)?;
    let asset = tool.assets.get(&target).ok_or(StatusCode::NOT_FOUND)?;

    // Cache: {cache_dir}/tools/{tag}/{filename}
    let cache_path = cache_dir()
        .join("tools")
        .join(&release.tag)
        .join(&asset.filename);

    if !cache_path.exists() {
        let client = reqwest::Client::new();
        let resp = github_request(&client, &asset.download_url)
            .send()
            .await
            .map_err(|e| {
                tracing::error!("failed to download {}: {e}", asset.download_url);
                StatusCode::BAD_GATEWAY
            })?;
        if !resp.status().is_success() {
            tracing::error!(
                "download failed: {} returned {}",
                asset.download_url,
                resp.status()
            );
            return Err(StatusCode::BAD_GATEWAY);
        }

        let bytes = resp.bytes().await.map_err(ise)?;
        if let Some(parent) = cache_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(ise)?;
        }
        tokio::fs::write(&cache_path, &bytes).await.map_err(ise)?;
        tracing::info!("cached {} ({} bytes)", asset.filename, bytes.len());
    }

    let bytes = tokio::fs::read(&cache_path).await.map_err(ise)?;

    let content_type = if asset.filename.ends_with(".zip") {
        "application/zip"
    } else {
        "application/octet-stream"
    };

    Response::builder()
        .header("content-type", content_type)
        .header(
            "content-disposition",
            format!("attachment; filename=\"{}\"", asset.filename),
        )
        .body(axum::body::Body::from(bytes))
        .map_err(ise)
}
