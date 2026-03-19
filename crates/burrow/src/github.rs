use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use axum::http::StatusCode;
use reqwest::Client;
use serde_json::Value;

use crate::models::GithubCommitJson;
use crate::{ApiResult, ise};

static GITHUB_COMMIT_CACHE: OnceLock<Mutex<HashMap<String, GithubCommitJson>>> = OnceLock::new();

pub fn github_commit_cache() -> &'static Mutex<HashMap<String, GithubCommitJson>> {
    GITHUB_COMMIT_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn github_owner_repo(upstream: &str) -> Option<(String, String)> {
    let trimmed = upstream.trim().trim_end_matches('/');

    // Normalize common schemes.
    let no_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .or_else(|| trimmed.strip_prefix("ssh://"))
        .or_else(|| trimmed.strip_prefix("git://"))
        .unwrap_or(trimmed);

    // Support git@github.com:org/repo(.git) style.
    let normalized = if let Some(rest) = no_scheme.strip_prefix("git@") {
        rest.replacen(':', "/", 1)
    } else {
        no_scheme.to_string()
    };

    let host_rest = normalized.strip_prefix("github.com/")?;
    let mut parts = host_rest.split('/');

    let owner = parts.next()?.trim();
    let repo_raw = parts.next()?.trim();

    if owner.is_empty() || repo_raw.is_empty() {
        return None;
    }

    let repo = repo_raw.strip_suffix(".git").unwrap_or(repo_raw).trim();
    if repo.is_empty() {
        return None;
    }

    Some((owner.to_string(), repo.to_string()))
}

pub fn github_cache_key(owner: &str, repo: &str, sha: &str) -> String {
    format!("{owner}/{repo}:{sha}")
}

pub async fn fetch_github_commit(
    client: &Client,
    owner: &str,
    repo: &str,
    sha: &str,
) -> ApiResult<GithubCommitJson> {
    let url = format!("https://api.github.com/repos/{owner}/{repo}/commits/{sha}");
    let mut request = client.get(url).header("User-Agent", "wezel-burrow");

    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let token = token.trim();
        if !token.is_empty() {
            request = request.bearer_auth(token);
        }
    }

    let response = request.send().await.map_err(|e| {
        tracing::error!("github fetch error: {:?}", e);
        StatusCode::BAD_GATEWAY
    })?;
    let status = response.status();

    if !status.is_success() {
        let code = StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        return Err(code);
    }

    let body: Value = response.json().await.map_err(|e| {
        tracing::error!("github response parse error: {:?}", e);
        StatusCode::BAD_GATEWAY
    })?;

    let full_sha = body
        .get("sha")
        .and_then(|v| v.as_str())
        .unwrap_or(sha)
        .to_string();
    let short_sha = full_sha.chars().take(7).collect::<String>();

    let author = body
        .get("author")
        .and_then(|v| v.get("login"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            body.get("commit")
                .and_then(|v| v.get("author"))
                .and_then(|v| v.get("name"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("unknown")
        .to_string();

    let message = body
        .get("commit")
        .and_then(|v| v.get("message"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let timestamp = body
        .get("commit")
        .and_then(|v| v.get("author"))
        .and_then(|v| v.get("date"))
        .and_then(|v| v.as_str())
        .or_else(|| {
            body.get("commit")
                .and_then(|v| v.get("committer"))
                .and_then(|v| v.get("date"))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string();

    let html_url = body
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    Ok(GithubCommitJson {
        sha: full_sha,
        short_sha,
        author,
        message,
        timestamp,
        html_url,
    })
}

pub async fn get_or_fetch_github_commit(
    client: &Client,
    owner: &str,
    repo: &str,
    sha: &str,
) -> ApiResult<GithubCommitJson> {
    let key = github_cache_key(owner, repo, sha);

    if let Some(cached) = github_commit_cache()
        .lock()
        .map_err(ise)?
        .get(&key)
        .cloned()
    {
        return Ok(cached);
    }

    let commit = fetch_github_commit(client, owner, repo, sha).await?;
    github_commit_cache()
        .lock()
        .map_err(ise)?
        .insert(key, commit.clone());

    Ok(commit)
}

/// Generic authenticated GitHub API call.
pub async fn github_api<T: serde::de::DeserializeOwned>(
    client: &Client,
    method: reqwest::Method,
    url: &str,
    token: &str,
    body: Option<Value>,
) -> ApiResult<T> {
    let mut req = client
        .request(method, url)
        .header("User-Agent", "wezel-burrow")
        .header("Accept", "application/vnd.github+json")
        .bearer_auth(token);
    if let Some(b) = body {
        req = req.json(&b);
    }
    let resp = req.send().await.map_err(|e| {
        tracing::error!("github api error: {:?}", e);
        StatusCode::BAD_GATEWAY
    })?;
    if !resp.status().is_success() {
        let code = resp.status().as_u16();
        return Err(StatusCode::from_u16(code).unwrap_or(StatusCode::BAD_GATEWAY));
    }
    resp.json().await.map_err(|e| {
        tracing::error!("github api parse error: {:?}", e);
        StatusCode::BAD_GATEWAY
    })
}
