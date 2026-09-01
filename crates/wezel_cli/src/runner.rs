//! HTTP client for the burrow run queue, used by `wezel experiment next`.
//!
//! Wraps the authenticated endpoints behind burrow's `ApiTokenAuth`:
//!   - `POST  /api/runs/claim`        — claim the next pending run for a repo
//!   - `POST  /api/runs/report`       — report measurements for a claimed run
//!   - `PATCH /api/runs/{id}/status`  — mark a run `complete` or `failed`
//!
//! Every request carries `Authorization: Bearer <wez_live_…>`. `report` stores
//! measurements and packaged attachments; burrow leaves the row `running`, so
//! the caller marks it `complete` via `set_status` afterwards.

use std::collections::HashSet;
use std::io::Cursor;

use anyhow::{Context, Result, bail};
use wezel_bench::run::CompletedRunAttachment;
use wezel_types::{
    ExperimentRun, ExperimentRunClaim, ExperimentRunReport, ExperimentRunResponse, RunBacklink,
};

const RUN_REPORT_PACKAGE_CONTENT_TYPE: &str = "application/vnd.wezel.run-report+tar+zstd";

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
    /// `backlink` records where this runner is executing, so a run that dies
    /// before it can report still points at its own logs. Beyond that the body
    /// carries nothing: the project-scoped `wez_live_` token tells burrow which
    /// queue to drain, so the client never computes (or could spoof) a repo
    /// upstream.
    pub fn claim(&self, backlink: Option<RunBacklink>) -> Result<Option<ExperimentRun>> {
        let url = format!("{}/api/runs/claim", self.base);
        let resp = self
            .agent
            .post(&url)
            .set("Authorization", &self.bearer())
            .send_json(ExperimentRunClaim { backlink })
            .map_err(|e| describe(e, "POST /api/runs/claim"))?;
        if resp.status() == 204 {
            return Ok(None);
        }
        let run = resp
            .into_json::<ExperimentRun>()
            .context("parsing /api/runs/claim response")?;
        Ok(Some(run))
    }

    /// Report measurements, summary definitions, and any packaged attachments
    /// for a claimed run.
    pub fn report(
        &self,
        report: &ExperimentRunReport,
        attachments: &[CompletedRunAttachment],
    ) -> Result<ExperimentRunResponse> {
        let url = format!("{}/api/runs/report", self.base);
        let package = build_report_package(report, attachments)?;
        let resp = self
            .agent
            .post(&url)
            .set("Authorization", &self.bearer())
            .set("Content-Type", RUN_REPORT_PACKAGE_CONTENT_TYPE)
            .send_bytes(&package)
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

/// Where this runner should say it ran: `--backlink`, else `WEZEL_RUN_BACKLINK`.
///
/// The env var is how a harness that wraps this command — a CI action, a runner
/// daemon — passes down a URL it alone can construct. Deliberately no sniffing
/// of any particular CI's variables: knowing how to describe itself is the
/// harness's job, not wezel's.
pub fn resolve_backlink(url: Option<String>, label: Option<String>) -> Option<RunBacklink> {
    Some(RunBacklink {
        url: url.or_else(|| env_nonempty("WEZEL_RUN_BACKLINK"))?,
        label: label.or_else(|| env_nonempty("WEZEL_RUN_BACKLINK_LABEL")),
    })
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn build_report_package(
    report: &ExperimentRunReport,
    attachments: &[CompletedRunAttachment],
) -> Result<Vec<u8>> {
    let encoder = zstd::stream::Encoder::new(Vec::new(), 3).context("creating zstd encoder")?;
    let mut archive = tar::Builder::new(encoder);

    let report_bytes = serde_json::to_vec_pretty(report).context("serializing report.json")?;
    let mut header = tar::Header::new_gnu();
    header
        .set_path("report.json")
        .context("setting report.json path")?;
    header.set_size(u64::try_from(report_bytes.len()).context("report.json too large")?);
    header.set_mode(0o644);
    header.set_cksum();
    archive
        .append(&header, Cursor::new(report_bytes))
        .context("adding report.json to report package")?;

    let mut seen = HashSet::new();
    for attachment in attachments {
        if !seen.insert(attachment.archive_path.as_str()) {
            bail!("duplicate report package path {}", attachment.archive_path);
        }
        archive
            .append_path_with_name(&attachment.source_path, &attachment.archive_path)
            .with_context(|| {
                format!(
                    "adding {} as {}",
                    attachment.source_path.display(),
                    attachment.archive_path
                )
            })?;
    }

    let encoder = archive.into_inner().context("finishing report archive")?;
    encoder.finish().context("finishing zstd report package")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_package_contains_manifest_and_attachments() {
        let dir = std::env::temp_dir().join(format!(
            "wezel-runner-package-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let source_path = dir.join("log.txt");
        std::fs::write(&source_path, b"hello").unwrap();

        let report = ExperimentRunReport {
            run_id: 7,
            steps: Vec::new(),
            summaries: Vec::new(),
        };
        let package = build_report_package(
            &report,
            &[CompletedRunAttachment {
                archive_path: "attachments/000-build/0/000-log.txt".into(),
                source_path,
            }],
        )
        .unwrap();

        let decoder = zstd::stream::read::Decoder::new(Cursor::new(package)).unwrap();
        let mut archive = tar::Archive::new(decoder);
        let names: Vec<_> = archive
            .entries()
            .unwrap()
            .map(|entry| {
                entry
                    .unwrap()
                    .path()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();

        assert_eq!(
            names,
            ["report.json", "attachments/000-build/0/000-log.txt"]
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
