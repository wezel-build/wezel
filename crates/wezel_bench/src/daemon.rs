use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize;
use serde::{Deserialize, Serialize};
use wezel_types::{ForagerJob, ForagerRunReport};

use std::collections::HashMap;

use crate::Config;
use crate::git;
use crate::run::{BurrowSession, run_experiment};

// ── Status file ───────────────────────────────────────────────────────────────

fn status_dir() -> PathBuf {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("forager")
}

fn status_path() -> PathBuf {
    status_dir().join("daemon.json")
}

#[derive(Debug, Serialize, Deserialize)]
struct DaemonStatus {
    pid: u32,
    upstream: String,
    server_url: String,
    started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    current_job: Option<JobStatus>,
    #[serde(default)]
    recent: Vec<JobStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JobStatus {
    id: u64,
    experiment: String,
    commit_sha: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn write_status(status: &DaemonStatus) {
    let dir = status_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(status_path(), serde_json::to_string_pretty(status).unwrap());
}

fn read_status() -> Option<DaemonStatus> {
    let raw = std::fs::read_to_string(status_path()).ok()?;
    serde_json::from_str(&raw).ok()
}

fn clear_status() {
    let _ = std::fs::remove_file(status_path());
}

fn now_rfc3339() -> String {
    // Use `date` to get an ISO 8601 timestamp without pulling in chrono.
    std::process::Command::new("date")
        .arg("+%Y-%m-%dT%H:%M:%S%z")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

const MAX_RECENT: usize = 20;

// ── Start ─────────────────────────────────────────────────────────────────────

pub fn run_start(
    repo_dir: &Path,
    poll_interval: u64,
    plugins: &HashMap<String, String>,
) -> Result<()> {
    let config = Config::load(repo_dir)?;
    let Some(ref server_url) = config.server_url else {
        bail!(
            "server_url not configured — set WEZEL_BURROW_URL or add server_url to .wezel/config.toml (or use --standalone mode)"
        );
    };
    let burrow = BurrowSession::new(server_url);
    let project_upstream = git::upstream(repo_dir)?;

    let mut status = DaemonStatus {
        pid: std::process::id(),
        upstream: project_upstream.clone(),
        server_url: server_url.clone(),
        started_at: now_rfc3339(),
        current_job: None,
        recent: Vec::new(),
    };
    write_status(&status);

    let queue_agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();

    log::info!(
        "forager daemon: upstream={} poll_interval={}s",
        project_upstream,
        poll_interval
    );

    let result = run_loop(
        &queue_agent,
        server_url,
        &burrow,
        &project_upstream,
        repo_dir,
        poll_interval,
        &mut status,
        plugins,
    );

    // Clean up status file on exit.
    clear_status();
    result
}

#[expect(clippy::too_many_arguments)]
fn run_loop(
    queue_agent: &ureq::Agent,
    server_url: &str,
    burrow: &BurrowSession,
    project_upstream: &str,
    repo_dir: &Path,
    poll_interval: u64,
    status: &mut DaemonStatus,
    plugins: &HashMap<String, String>,
) -> Result<()> {
    loop {
        let next_body = serde_json::json!({ "project_upstream": project_upstream });
        let response = queue_agent
            .post(&format!("{}/api/forager/jobs/next", server_url))
            .send_json(&next_body)
            .context("polling for next job")?;

        if response.status() == 204 {
            status.current_job = None;
            write_status(status);
            log::debug!("no pending jobs; sleeping {}s", poll_interval);
            std::thread::sleep(std::time::Duration::from_secs(poll_interval));
            continue;
        }

        let job: ForagerJob = response.into_json().context("parsing job response")?;
        let job_id = job.id;
        log::info!(
            "claimed queue job {}: sha={} experiment={}",
            job_id,
            &job.commit_sha[..7.min(job.commit_sha.len())],
            job.experiment_name
        );

        status.current_job = Some(JobStatus {
            id: job_id,
            experiment: job.experiment_name.clone(),
            commit_sha: job.commit_sha.clone(),
            status: "running".to_string(),
            error: None,
        });
        write_status(status);

        git::reset_worktree(repo_dir)
            .with_context(|| format!("resetting worktree before job {}", job_id))?;
        git::fetch(repo_dir).with_context(|| format!("git fetch before job {}", job_id))?;
        git::checkout_detached(repo_dir, &job.commit_sha)
            .with_context(|| format!("checkout {} for job {}", job.commit_sha, job_id))?;

        let result = run_experiment(&job.experiment_name, repo_dir, plugins);

        // Submit results to Burrow and update the queue job status.
        let (patch_body, finished) = match result {
            Ok((ref step_reports, ref summaries)) => {
                let report = ForagerRunReport {
                    token: job.token.clone(),
                    steps: step_reports.clone(),
                    summaries: summaries.clone(),
                    bisection_id: job.bisection_id,
                };
                if let Err(e) = burrow.submit(&report) {
                    log::warn!("failed to submit run report: {e:#}");
                }
                (
                    serde_json::json!({ "status": "complete" }),
                    JobStatus {
                        id: job_id,
                        experiment: job.experiment_name.clone(),
                        commit_sha: job.commit_sha.clone(),
                        status: "complete".to_string(),
                        error: None,
                    },
                )
            }
            Err(ref e) => (
                serde_json::json!({ "status": "failed", "error": format!("{e:#}") }),
                JobStatus {
                    id: job_id,
                    experiment: job.experiment_name.clone(),
                    commit_sha: job.commit_sha.clone(),
                    status: "failed".to_string(),
                    error: Some(format!("{e:#}")),
                },
            ),
        };

        queue_agent
            .patch(&format!("{}/api/forager/jobs/{}", server_url, job_id))
            .send_json(&patch_body)
            .with_context(|| format!("patching job {} status", job_id))?;

        if let Err(ref e) = result {
            log::warn!("job {} failed: {e:#}", job_id);
        } else {
            log::info!("job {} complete", job_id);
        }

        status.current_job = None;
        status.recent.push(finished);
        if status.recent.len() > MAX_RECENT {
            status.recent.remove(0);
        }
        write_status(status);
    }
}

// ── Status ────────────────────────────────────────────────────────────────────

pub fn run_status() -> Result<()> {
    let Some(status) = read_status() else {
        println!("{}", "No daemon running.".dimmed());
        return Ok(());
    };

    // Check if the PID is actually alive.
    let alive = std::process::Command::new("kill")
        .args(["-0", &status.pid.to_string()])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success());
    if !alive {
        println!(
            "{} (stale status file, pid {} not running)",
            "No daemon running.".dimmed(),
            status.pid
        );
        clear_status();
        return Ok(());
    }

    println!(
        "{} {} {}",
        "Daemon".bold(),
        format!("pid {}", status.pid).dimmed(),
        "running".green().bold(),
    );
    println!("  {} {}", "upstream:".dimmed(), status.upstream,);
    println!("  {}  {}", "burrow:".dimmed(), status.server_url,);
    println!("  {} {}", "started:".dimmed(), status.started_at,);

    println!();
    match &status.current_job {
        Some(job) => {
            println!("{}", "Current job:".bold());
            println!(
                "  #{} {} @ {}",
                job.id,
                job.experiment.bold(),
                &job.commit_sha[..7.min(job.commit_sha.len())],
            );
        }
        None => {
            println!("{}", "Idle — waiting for jobs.".dimmed());
        }
    }

    if !status.recent.is_empty() {
        println!();
        println!("{}", "Recent jobs:".bold());
        for job in status.recent.iter().rev() {
            let status_label = match job.status.as_str() {
                "complete" => "complete".green().to_string(),
                "failed" => "failed".red().to_string(),
                other => other.to_string(),
            };
            let sha = &job.commit_sha[..7.min(job.commit_sha.len())];
            print!(
                "  #{} {} @ {} — {status_label}",
                job.id, job.experiment, sha
            );
            if let Some(ref e) = job.error {
                print!(" ({})", e.dimmed());
            }
            println!();
        }
    }

    Ok(())
}
