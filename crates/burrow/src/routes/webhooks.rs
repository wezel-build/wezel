use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use sqlx::PgPool;

use crate::{ApiResult, ise};

type HmacSha256 = Hmac<Sha256>;

// ── GitHub push event payload (subset) ──────────────────────────────────────

#[derive(serde::Deserialize)]
struct PushEvent {
    #[serde(rename = "ref")]
    git_ref: String,
    before: String,
    after: String,
    repository: PushRepo,
    #[serde(default)]
    commits: Vec<PushCommit>,
}

#[derive(serde::Deserialize)]
struct PushRepo {
    clone_url: Option<String>,
    ssh_url: Option<String>,
    html_url: Option<String>,
}

#[derive(serde::Deserialize)]
struct PushCommit {
    id: String,
    message: Option<String>,
    timestamp: Option<String>,
    author: Option<PushAuthor>,
}

#[derive(serde::Deserialize)]
struct PushAuthor {
    username: Option<String>,
    name: Option<String>,
}

// ── Signature verification ──────────────────────────────────────────────────

fn verify_signature(secret: &str, payload: &[u8], header: &str) -> bool {
    let Some(hex_sig) = header.strip_prefix("sha256=") else {
        return false;
    };
    let Ok(expected) = hex::decode(hex_sig) else {
        return false;
    };
    let Ok(mut mac) = HmacSha256::new_from_slice(secret.as_bytes()) else {
        return false;
    };
    mac.update(payload);
    mac.verify_slice(&expected).is_ok()
}

// ── Normalize repo URL to match repos.upstream ──────────────────────────────

fn normalize_repo_url(push_repo: &PushRepo) -> Option<String> {
    // Try html_url first (most stable), then clone_url, then ssh_url.
    push_repo
        .html_url
        .as_deref()
        .or(push_repo.clone_url.as_deref())
        .or(push_repo.ssh_url.as_deref())
        .map(|u| u.trim_end_matches('/').trim_end_matches(".git").to_string())
}

// ── Handler ─────────────────────────────────────────────────────────────────

/// `POST /api/webhooks/github`
///
/// Receives a GitHub push webhook. Validates the signature against the
/// matching repo's `webhook_secret`, then:
/// 1. Creates commit rows for each commit in the push (with parent linkage).
/// 2. Upserts the branch head in the `branches` table.
pub async fn post_github_webhook(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> ApiResult<StatusCode> {
    // Only handle push events.
    let event_type = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if event_type != "push" {
        // Acknowledge non-push events (ping, etc.) without processing.
        return Ok(StatusCode::OK);
    }

    let payload: PushEvent = serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;

    // Resolve repo row from the push payload URL.
    let repo_url = normalize_repo_url(&payload.repository).ok_or(StatusCode::BAD_REQUEST)?;

    let repo: Option<(i64, Option<String>)> = sqlx::query_as(
        "SELECT id, webhook_secret FROM repos WHERE upstream = $1 \
         OR upstream = $2 \
         OR upstream = $3",
    )
    .bind(&repo_url)
    .bind(format!("{repo_url}.git"))
    .bind(format!("{repo_url}/"))
    .fetch_optional(&pool)
    .await
    .map_err(ise)?;

    let (repo_id, webhook_secret) = repo.ok_or(StatusCode::NOT_FOUND)?;

    // Validate signature if the repo has a webhook secret configured.
    if let Some(secret) = &webhook_secret {
        let sig_header = headers
            .get("x-hub-signature-256")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !verify_signature(secret, &body, sig_header) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    // Extract branch name from ref (e.g. "refs/heads/main" → "main").
    let branch = payload
        .git_ref
        .strip_prefix("refs/heads/")
        .ok_or(StatusCode::OK)?; // tag pushes etc — just acknowledge

    // Insert commits from the push payload.
    // GitHub sends commits in chronological order; each has its parent in the
    // array or as `before` for the first one.
    for (i, commit) in payload.commits.iter().enumerate() {
        let parent_sha = if i == 0 {
            // First commit's parent is `before` (the previous HEAD).
            if payload.before.starts_with("0000000") {
                None // new branch, no parent
            } else {
                Some(payload.before.as_str())
            }
        } else {
            Some(payload.commits[i - 1].id.as_str())
        };

        let sha = &commit.id;
        let short_sha: String = sha.chars().take(7).collect();

        let author = commit
            .author
            .as_ref()
            .and_then(|a| a.username.as_deref().or(a.name.as_deref()))
            .unwrap_or("");

        let message = commit.message.as_deref().unwrap_or("");
        let timestamp = commit.timestamp.as_deref().unwrap_or("");

        sqlx::query(
            "INSERT INTO commits (repo_id, sha, short_sha, parent_sha, author, message, timestamp) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) \
             ON CONFLICT (repo_id, sha) DO UPDATE SET \
                parent_sha = COALESCE(commits.parent_sha, EXCLUDED.parent_sha), \
                author = CASE WHEN commits.author = '' THEN EXCLUDED.author ELSE commits.author END, \
                message = CASE WHEN commits.message = '' THEN EXCLUDED.message ELSE commits.message END, \
                timestamp = CASE WHEN commits.timestamp = '' THEN EXCLUDED.timestamp ELSE commits.timestamp END",
        )
        .bind(repo_id)
        .bind(sha)
        .bind(&short_sha)
        .bind(parent_sha)
        .bind(author)
        .bind(message)
        .bind(timestamp)
        .execute(&pool)
        .await
        .map_err(ise)?;
    }

    // If the commits array is empty (force-push with no new commits), still
    // ensure the `after` commit exists.
    if payload.commits.is_empty() && !payload.after.starts_with("0000000") {
        let short_sha: String = payload.after.chars().take(7).collect();
        let parent_sha = if payload.before.starts_with("0000000") {
            None
        } else {
            Some(payload.before.as_str())
        };

        sqlx::query(
            "INSERT INTO commits (repo_id, sha, short_sha, parent_sha, author, message, timestamp) \
             VALUES ($1, $2, $3, $4, '', '', '') \
             ON CONFLICT (repo_id, sha) DO NOTHING",
        )
        .bind(repo_id)
        .bind(&payload.after)
        .bind(&short_sha)
        .bind(parent_sha)
        .execute(&pool)
        .await
        .map_err(ise)?;
    }

    // Upsert branch head (skip for branch deletions where after is all zeros).
    if !payload.after.starts_with("0000000") {
        sqlx::query(
            "INSERT INTO branches (repo_id, name, head_sha, updated_at) \
             VALUES ($1, $2, $3, now()) \
             ON CONFLICT (repo_id, name) DO UPDATE SET \
                head_sha = EXCLUDED.head_sha, \
                updated_at = now()",
        )
        .bind(repo_id)
        .bind(branch)
        .bind(&payload.after)
        .execute(&pool)
        .await
        .map_err(ise)?;
    } else {
        // Branch deletion — remove the branch row.
        sqlx::query("DELETE FROM branches WHERE repo_id = $1 AND name = $2")
            .bind(repo_id)
            .bind(branch)
            .execute(&pool)
            .await
            .map_err(ise)?;
    }

    tracing::info!(
        repo_id,
        branch,
        after = %payload.after,
        commits = payload.commits.len(),
        "processed github push webhook"
    );

    Ok(StatusCode::OK)
}
