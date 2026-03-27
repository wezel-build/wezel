use std::path::Path;

use anyhow::{Context, Result, bail};
use wezel_types::{ForagerJob, ForagerRunReport, ForagerStepReport};

use crate::{BenchmarkToml, Config, invoke_forager, parse_benchmark};
use crate::git;

pub struct BurrowSession {
    agent: ureq::Agent,
    server_url: String,
}

impl BurrowSession {
    pub fn from_config(config: &Config) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .build(),
            server_url: config.server_url.clone(),
        }
    }

    pub fn claim(
        &self,
        project_upstream: &str,
        commit_sha: &str,
        benchmark_name: &str,
        commit_author: &str,
        commit_message: &str,
        commit_timestamp: &str,
    ) -> Result<ForagerJob> {
        let claim_body = serde_json::json!({
            "project_upstream": project_upstream,
            "commit_sha": commit_sha,
            "benchmark_name": benchmark_name,
            "commit_author": commit_author,
            "commit_message": commit_message,
            "commit_timestamp": commit_timestamp,
        });

        let job: ForagerJob = self
            .agent
            .post(&format!("{}/api/forager/claim", self.server_url))
            .send_json(&claim_body)
            .context("claiming job from Burrow")?
            .into_json()
            .context("parsing claim response")?;

        log::info!(
            "job claimed (token: {})",
            &job.token[..8.min(job.token.len())]
        );
        Ok(job)
    }

    pub fn submit(&self, report: &ForagerRunReport) -> Result<()> {
        self.agent
            .post(&format!("{}/api/forager/run", self.server_url))
            .send_json(report)
            .context("submitting run report to Burrow")?;
        Ok(())
    }
}

pub fn list_benchmarks(project_dir: &Path) -> Result<()> {
    let benchmarks_dir = project_dir.join(".wezel").join("benchmarks");
    if !benchmarks_dir.is_dir() {
        bail!("no benchmarks directory at {}", benchmarks_dir.display());
    }

    let mut found = Vec::new();
    for entry in std::fs::read_dir(&benchmarks_dir).context("reading benchmarks directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir()
            && path.join("benchmark.toml").is_file()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
        {
            let toml_path = path.join("benchmark.toml");
            let description = std::fs::read_to_string(&toml_path)
                .ok()
                .and_then(|raw| toml::from_str::<BenchmarkToml>(&raw).ok())
                .and_then(|b| b.description);
            found.push((name.to_string(), description));
        }
    }

    if found.is_empty() {
        println!("No benchmarks found in {}", benchmarks_dir.display());
        return Ok(());
    }

    found.sort_by(|a, b| a.0.cmp(&b.0));
    println!("Available benchmarks:\n");
    for (name, desc) in &found {
        match desc {
            Some(d) => println!("  {name}  — {d}"),
            None => println!("  {name}"),
        }
    }
    println!("\nRun with: forager run -b <name>");

    Ok(())
}

pub fn run_benchmark(
    benchmark_name: &str,
    project_dir: &Path,
    burrow: Option<&BurrowSession>,
) -> Result<()> {
    let benchmark_dir = project_dir
        .join(".wezel")
        .join("benchmarks")
        .join(benchmark_name);

    if !benchmark_dir.is_dir() {
        bail!("benchmark directory not found: {}", benchmark_dir.display());
    }

    let (_name, _description, steps) = parse_benchmark(&benchmark_dir)?;

    // Detect current commit info from git.
    let commit_sha = git::current_sha(project_dir)?;
    let project_upstream = git::upstream(project_dir)?;
    let commit_author = git::commit_author(project_dir);
    let commit_message = git::commit_message(project_dir);
    let commit_timestamp = git::commit_timestamp(project_dir);

    // Claim from Burrow if we have a session.
    let job = match burrow {
        Some(b) => {
            log::info!(
                "claiming job: upstream={} sha={} benchmark={}",
                project_upstream,
                &commit_sha[..7.min(commit_sha.len())],
                benchmark_name
            );
            Some(b.claim(
                &project_upstream,
                &commit_sha,
                benchmark_name,
                &commit_author,
                &commit_message,
                &commit_timestamp,
            )?)
        }
        None => None,
    };

    // Run each step.
    let mut step_reports: Vec<ForagerStepReport> = Vec::new();

    for step in &steps {
        log::info!("step '{}' [forager={}]", step.name, step.forager);

        // Apply patch if the step declares one.
        if let Some(ref patch_stem) = step.diff {
            let patch_path = benchmark_dir.join(format!("{patch_stem}.patch"));
            log::info!("  applying patch: {}", patch_path.display());
            git::apply_patch(project_dir, &patch_path)
                .with_context(|| format!("applying patch for step '{}'", step.name))?;
        }

        // Invoke the forager plugin.
        let measurement = invoke_forager(&step.forager, &step.name, &step.inputs, project_dir);

        match measurement {
            Ok(m) => {
                step_reports.push(ForagerStepReport {
                    step: step.name.clone(),
                    measurement: m,
                });
            }
            Err(e) if e.is_hard() => bail!("{e}"),
            Err(e) => {
                log::warn!("{e}");
                step_reports.push(ForagerStepReport {
                    step: step.name.clone(),
                    measurement: None,
                });
            }
        }
    }

    // Print results locally.
    println!("Benchmark: {benchmark_name}");
    println!("Commit:    {}", &commit_sha[..7.min(commit_sha.len())]);
    for report in &step_reports {
        match &report.measurement {
            Some(m) => println!(
                "  {} — {} = {} {}",
                report.step,
                m.name,
                m.value,
                m.unit.as_deref().unwrap_or("")
            ),
            None => println!("  {} — (no measurement)", report.step),
        }
    }

    // Submit to Burrow if we have a session.
    if let Some(job) = job {
        let report = ForagerRunReport {
            token: job.token,
            steps: step_reports,
        };
        burrow.unwrap().submit(&report)?;
        println!("Results submitted to Burrow.");
    }

    Ok(())
}
