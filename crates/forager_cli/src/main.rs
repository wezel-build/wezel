use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use figment::Figment;
use figment::providers::{Format, Serialized, Toml};
use serde::{Deserialize, Serialize};
use wezel_types::{
    ForagerJob, ForagerPluginEnvelope, ForagerQueueJob, ForagerRunReport, ForagerStepReport,
};

// ── Config ────────────────────────────────────────────────────────────────────

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProjectConfig {
    pub server_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    pub server_url: String,
}

fn load_config(project_dir: &Path) -> Result<Config> {
    let config_path = project_dir.join(".wezel").join("config.toml");
    if !config_path.is_file() {
        bail!("no .wezel/config.toml found at {}", config_path.display());
    }
    let defaults = ProjectConfig { server_url: None };
    let resolved: ProjectConfig = Figment::new()
        .merge(Serialized::defaults(defaults))
        .merge(Toml::file(&config_path))
        .extract()
        .with_context(|| format!("loading config from {}", config_path.display()))?;
    let server_url = resolved
        .server_url
        .filter(|s| !s.is_empty())
        .with_context(|| format!("server_url not set in {}", config_path.display()))?;
    Ok(Config { server_url })
}

// ── Scenario TOML parsing ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct BenchmarkToml {
    name: String,
    description: Option<String>,
    steps: Vec<BenchmarkStepToml>,
}

#[derive(Debug, Deserialize)]
struct BenchmarkStepToml {
    name: String,
    tool: Option<String>,
    description: Option<String>,
    diff: Option<String>,
    #[serde(flatten)]
    rest: HashMap<String, toml::Value>,
}

struct ParsedStep {
    name: String,
    forager: String,
    #[allow(dead_code)]
    description: Option<String>,
    diff: Option<String>,
    inputs: serde_json::Value,
}

fn parse_benchmark(benchmark_dir: &Path) -> Result<(String, Option<String>, Vec<ParsedStep>)> {
    let toml_path = benchmark_dir.join("benchmark.toml");
    let raw = std::fs::read_to_string(&toml_path)
        .with_context(|| format!("reading {}", toml_path.display()))?;
    let scenario: BenchmarkToml =
        toml::from_str(&raw).with_context(|| format!("parsing {}", toml_path.display()))?;

    let mut steps = Vec::with_capacity(scenario.steps.len());
    for raw_step in scenario.steps {
        let forager = match raw_step.tool {
            Some(f) => f,
            None if raw_step.rest.contains_key("cmd") => "exec".to_string(),
            None => bail!("step '{}' has no tool name and no cmd field", raw_step.name),
        };

        let inputs_map: serde_json::Map<String, serde_json::Value> = raw_step
            .rest
            .into_iter()
            .map(|(k, v)| Ok((k, toml_to_json(v)?)))
            .collect::<Result<_>>()?;

        steps.push(ParsedStep {
            name: raw_step.name,
            forager,
            description: raw_step.description,
            diff: raw_step.diff,
            inputs: serde_json::Value::Object(inputs_map),
        });
    }

    Ok((scenario.name, scenario.description, steps))
}

fn toml_to_json(v: toml::Value) -> Result<serde_json::Value> {
    Ok(match v {
        toml::Value::String(s) => serde_json::Value::String(s),
        toml::Value::Integer(i) => serde_json::json!(i),
        toml::Value::Float(f) => serde_json::json!(f),
        toml::Value::Boolean(b) => serde_json::Value::Bool(b),
        toml::Value::Array(a) => serde_json::Value::Array(
            a.into_iter()
                .map(toml_to_json)
                .collect::<Result<Vec<_>>>()?,
        ),
        toml::Value::Table(t) => serde_json::Value::Object(
            t.into_iter()
                .map(|(k, v)| Ok((k, toml_to_json(v)?)))
                .collect::<Result<serde_json::Map<_, _>>>()?,
        ),
        toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
    })
}

// ── Git helpers ───────────────────────────────────────────────────────────────

fn git_current_sha(project_dir: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(project_dir)
        .stderr(std::process::Stdio::null())
        .output()
        .context("running git rev-parse HEAD")?;
    if !out.status.success() {
        bail!("git rev-parse HEAD failed");
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn git_upstream(project_dir: &Path) -> Result<String> {
    let out = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(project_dir)
        .stderr(std::process::Stdio::null())
        .output()
        .context("running git remote get-url origin")?;
    if !out.status.success() {
        bail!("could not determine git remote origin");
    }
    let raw = String::from_utf8_lossy(&out.stdout).trim().to_string();
    Ok(normalize_upstream(&raw))
}

fn git_commit_author(project_dir: &Path) -> String {
    let out = Command::new("git")
        .args(["log", "-1", "--format=%an"])
        .current_dir(project_dir)
        .output();
    out.ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn git_commit_message(project_dir: &Path) -> String {
    let out = Command::new("git")
        .args(["log", "-1", "--format=%s"])
        .current_dir(project_dir)
        .output();
    out.ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn git_commit_timestamp(project_dir: &Path) -> String {
    let out = Command::new("git")
        .args(["log", "-1", "--format=%aI"])
        .current_dir(project_dir)
        .output();
    out.ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn git_apply_patch(project_dir: &Path, patch: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["apply", &patch.to_string_lossy()])
        .current_dir(project_dir)
        .status()
        .context("running git apply")?;
    if !status.success() {
        bail!("git apply {} failed", patch.display());
    }
    Ok(())
}

fn git_fetch(repo_dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .args(["fetch", "--quiet", "origin"])
        .current_dir(repo_dir)
        .status()
        .context("running git fetch")?;
    if !status.success() {
        bail!("git fetch failed");
    }
    Ok(())
}

fn git_checkout_detached(repo_dir: &Path, sha: &str) -> Result<()> {
    let status = Command::new("git")
        .args(["checkout", "--detach", sha])
        .current_dir(repo_dir)
        .status()
        .context("running git checkout --detach")?;
    if !status.success() {
        bail!("git checkout --detach {} failed", sha);
    }
    Ok(())
}

fn normalize_upstream(url: &str) -> String {
    let s = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("ssh://")
        .trim_start_matches("git://");
    let s = if let Some(rest) = s.strip_prefix("git@") {
        rest.replacen(':', "/", 1)
    } else {
        s.to_string()
    };
    s.trim_end_matches(".git").to_string()
}

// ── Forager plugin invocation ─────────────────────────────────────────────────

#[derive(Debug, thiserror::Error)]
enum StepError {
    #[error("`{binary}` not found on PATH — is it installed?")]
    PluginNotFound { binary: String },

    #[error("failed to spawn `{binary}`: {reason}")]
    SpawnFailed { binary: String, reason: String },

    #[error("step '{step}': `{binary}` exited with {status}")]
    PluginFailed {
        step: String,
        binary: String,
        status: std::process::ExitStatus,
    },

    #[error("step '{step}': `{binary}` did not write FORAGER_OUT")]
    NoOutput { step: String, binary: String },

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

impl StepError {
    fn is_hard(&self) -> bool {
        matches!(self, Self::PluginNotFound { .. } | Self::SpawnFailed { .. })
    }
}

fn invoke_forager(
    forager_name: &str,
    step_name: &str,
    inputs: &serde_json::Value,
    project_dir: &Path,
) -> std::result::Result<Option<wezel_types::ForagerPluginOutput>, StepError> {
    let binary_name = format!("forager-{forager_name}");
    // Look next to our own executable first, then fall back to PATH.
    let binary = std::env::current_exe()
        .ok()
        .and_then(|exe| {
            let sibling = exe.parent()?.join(&binary_name);
            sibling.is_file().then_some(sibling)
        })
        .or_else(|| which::which(&binary_name).ok())
        .ok_or_else(|| StepError::PluginNotFound {
            binary: binary_name.clone(),
        })?;

    // Write inputs to a temp file.
    let inputs_id = uuid::Uuid::new_v4();
    let inputs_path = std::env::temp_dir().join(format!("forager-inputs-{inputs_id}.json"));
    let out_path = std::env::temp_dir().join(format!("forager-out-{inputs_id}.json"));

    std::fs::write(&inputs_path, serde_json::to_string(inputs).unwrap())
        .map_err(|e| StepError::Other(anyhow::anyhow!("writing FORAGER_INPUTS: {e}")))?;

    let status = Command::new(&binary)
        .env("FORAGER_INPUTS", &inputs_path)
        .env("FORAGER_OUT", &out_path)
        .env("FORAGER_STEP", step_name)
        .current_dir(project_dir)
        .status()
        .map_err(|e| StepError::SpawnFailed {
            binary: binary_name.clone(),
            reason: e.to_string(),
        })?;

    if !status.success() {
        return Err(StepError::PluginFailed {
            step: step_name.to_string(),
            binary: binary_name,
            status,
        });
    }

    let envelope_raw = std::fs::read_to_string(&out_path).map_err(|_| StepError::NoOutput {
        step: step_name.to_string(),
        binary: binary_name.clone(),
    })?;

    let envelope: ForagerPluginEnvelope = serde_json::from_str(&envelope_raw)
        .map_err(|e| StepError::Other(anyhow::anyhow!("parsing output from {binary_name}: {e}")))?;

    // Best-effort cleanup of temp files.
    let _ = std::fs::remove_file(&inputs_path);
    let _ = std::fs::remove_file(&out_path);

    Ok(envelope.measurement)
}

// ── CLI ───────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "forager", about = "Wezel benchmark runner")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run a benchmark against the current checkout.
    Run {
        /// Benchmark name (matches .wezel/benchmarks/<name>/). Omit to list available benchmarks.
        #[arg(short, long)]
        benchmark: Option<String>,
        /// Project root directory (defaults to current directory).
        #[arg(long)]
        project_dir: Option<PathBuf>,
    },
    /// Poll burrow for queued jobs and run them.
    Serve {
        /// Path to the repository to check out and run benchmarks in.
        #[arg(long)]
        repo_dir: PathBuf,
        /// Seconds to wait between polls when no job is available.
        #[arg(long, default_value = "10")]
        poll_interval: u64,
    },
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Run {
            benchmark,
            project_dir,
        } => {
            let project_dir = project_dir
                .unwrap_or_else(|| std::env::current_dir().expect("getting current directory"));
            match benchmark {
                Some(name) => run_benchmark(&name, &project_dir, None),
                None => list_benchmarks(&project_dir),
            }
        }
        Cmd::Serve {
            repo_dir,
            poll_interval,
        } => run_serve(&repo_dir, poll_interval),
    }
}

fn run_serve(repo_dir: &Path, poll_interval: u64) -> Result<()> {
    let config = load_config(repo_dir)?;
    let burrow = BurrowSession::from_config(&config);
    let project_upstream = git_upstream(repo_dir)?;

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

        git_fetch(repo_dir).with_context(|| format!("git fetch before job {}", job.id))?;
        git_checkout_detached(repo_dir, &job.commit_sha)
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

fn list_benchmarks(project_dir: &Path) -> Result<()> {
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

struct BurrowSession {
    agent: ureq::Agent,
    server_url: String,
}

impl BurrowSession {
    fn from_config(config: &Config) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .build(),
            server_url: config.server_url.clone(),
        }
    }

    fn claim(
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

    fn submit(&self, report: &ForagerRunReport) -> Result<()> {
        self.agent
            .post(&format!("{}/api/forager/run", self.server_url))
            .send_json(report)
            .context("submitting run report to Burrow")?;
        Ok(())
    }
}

fn run_benchmark(
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
    let commit_sha = git_current_sha(project_dir)?;
    let project_upstream = git_upstream(project_dir)?;
    let commit_author = git_commit_author(project_dir);
    let commit_message = git_commit_message(project_dir);
    let commit_timestamp = git_commit_timestamp(project_dir);

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

        // Apply patch if one exists.
        let patch_stem = step.diff.as_deref().unwrap_or(&step.name);
        let patch_path = benchmark_dir.join(format!("{patch_stem}.patch"));
        if patch_path.is_file() {
            log::info!("  applying patch: {}", patch_path.display());
            git_apply_patch(project_dir, &patch_path)
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
