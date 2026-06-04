//! HTTP client for the burrow run queue, used by `wezel experiment next`.
//!
//! Wraps the authenticated endpoints behind burrow's `ApiTokenAuth`:
//!   - `POST  /api/runs/claim`        — claim the next pending run for a repo
//!   - `POST  /api/runs/report`       — report measurements for a claimed run
//!   - `PATCH /api/runs/{id}/status`  — mark a run `complete` or `failed`
//!
//! Every request carries `Authorization: Bearer <wez_live_…>`. `report` only
//! stores measurements; burrow leaves the row `running`, so the caller marks it
//! `complete` via `set_status` afterwards.

use anyhow::{Context, Result};
use wezel_types::{ExperimentRun, ExperimentRunReport, ExperimentRunResponse};

pub struct RunnerClient {
    agent: ureq::Agent,
    base: String,
    token: String,
}

impl RunnerClient {
    pub fn new(server_url: &str, token: &str) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .build(),
            base: server_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        }
    }

    fn bearer(&self) -> String {
        format!("Bearer {}", self.token)
    }

    /// Claim the next pending run for the token's project. Returns `Ok(None)`
    /// when the queue is empty (HTTP 204).
    ///
    /// The request carries no body: the project-scoped `wez_live_` token tells
    /// burrow which queue to drain, so the client never computes (or could
    /// spoof) a repo upstream.
    pub fn claim(&self) -> Result<Option<ExperimentRun>> {
        let url = format!("{}/api/runs/claim", self.base);
        let resp = self
            .agent
            .post(&url)
            .set("Authorization", &self.bearer())
            .call()
            .map_err(|e| describe(e, "POST /api/runs/claim"))?;
        if resp.status() == 204 {
            return Ok(None);
        }
        let run = resp
            .into_json::<ExperimentRun>()
            .context("parsing /api/runs/claim response")?;
        Ok(Some(run))
    }

    /// Report measurements and summary definitions for a claimed run.
    pub fn report(&self, report: &ExperimentRunReport) -> Result<ExperimentRunResponse> {
        let url = format!("{}/api/runs/report", self.base);
        let resp = self
            .agent
            .post(&url)
            .set("Authorization", &self.bearer())
            .send_json(report)
            .map_err(|e| describe(e, "POST /api/runs/report"))?;
        resp.into_json::<ExperimentRunResponse>()
            .context("parsing /api/runs/report response")
    }

    /// Mark a run `complete` or `failed`. `error` carries failure detail and is
    /// ignored by burrow on success.
    pub fn set_status(&self, run_id: u64, status: &str, error: Option<&str>) -> Result<()> {
        let url = format!("{}/api/runs/{run_id}/status", self.base);
        self.agent
            .request("PATCH", &url)
            .set("Authorization", &self.bearer())
            .send_json(serde_json::json!({ "status": status, "error": error }))
            .map_err(|e| describe(e, "PATCH /api/runs/{id}/status"))?;
        Ok(())
    }
}

/// Render a ureq error with the server's response body when it returned a
/// non-2xx status, so failures surface the actual reason.
fn describe(err: ureq::Error, what: &str) -> anyhow::Error {
    match err {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            anyhow::anyhow!("{what}: HTTP {code}: {}", body.trim())
        }
        ureq::Error::Transport(t) => anyhow::anyhow!("{what}: {t}"),
    }
}
