use std::path::Path;

use anyhow::{Context, Result};
use wezel_types::ForagerQueueJob;

use crate::Config;
use crate::git;
use crate::run::{BurrowSession, run_benchmark};

pub fn run_serve(repo_dir: &Path, poll_interval: u64) -> Result<()> {
    let config = Config::load(repo_dir)?;
    let burrow = BurrowSession::from_config(&config);
    let project_upstream = git::upstream(repo_dir)?;

    let queue_agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();

    log::info!(
        "forager serve: upstream={} poll_interval={}s",
        project_upstream,
        poll_interval
    );

    loop {
        let next_body = serde_json::json!({ "project_upstream": project_upstream });
        let response = queue_agent
            .post(&format!("{}/api/forager/jobs/next", config.server_url))
            .send_json(&next_body)
            .context("polling for next job")?;

        if response.status() == 204 {
            log::debug!("no pending jobs; sleeping {}s", poll_interval);
            std::thread::sleep(std::time::Duration::from_secs(poll_interval));
            continue;
        }

        let job: ForagerQueueJob = response.into_json().context("parsing job response")?;
        log::info!(
            "claimed queue job {}: sha={} benchmark={}",
            job.id,
            &job.commit_sha[..7.min(job.commit_sha.len())],
            job.benchmark_name
        );

        git::reset_worktree(repo_dir)
            .with_context(|| format!("resetting worktree before job {}", job.id))?;
        git::fetch(repo_dir).with_context(|| format!("git fetch before job {}", job.id))?;
        git::checkout_detached(repo_dir, &job.commit_sha)
            .with_context(|| format!("checkout {} for job {}", job.commit_sha, job.id))?;

        let result = run_benchmark(&job.benchmark_name, repo_dir, Some(&burrow));

        let patch_body = match result {
            Ok(()) => serde_json::json!({ "status": "complete" }),
            Err(ref e) => serde_json::json!({ "status": "failed", "error": format!("{e:#}") }),
        };

        queue_agent
            .patch(&format!(
                "{}/api/forager/jobs/{}",
                config.server_url, job.id
            ))
            .send_json(&patch_body)
            .with_context(|| format!("patching job {} status", job.id))?;

        if let Err(e) = result {
            log::warn!("job {} failed: {e:#}", job.id);
        } else {
            log::info!("job {} complete", job.id);
        }
        // No sleep — poll again immediately after handling a job.
    }
}
