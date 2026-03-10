use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use figment::Figment;
use figment::providers::{Format, Serialized, Toml};
use serde::{Deserialize, Serialize};
use wezel_types::{ForagerJob, ForagerPluginEnvelope, ForagerRunReport, ForagerStepReport};

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
        bail!(
            "no .wezel/config.toml found at {}",
            config_path.display()
        );
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
            None => bail!(
                "step '{}' has no tool name and no cmd field",
                raw_step.name
            ),
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

fn invoke_forager(
    forager_name: &str,
    step_name: &str,
    inputs: &serde_json::Value,
    project_dir: &Path,
) -> Result<Option<wezel_types::ForagerPluginOutput>> {
    let binary_name = format!("forager-{forager_name}");
    let binary = which::which(&binary_name)
        .with_context(|| format!("{binary_name} not found on PATH"))?;

    // Write inputs to a temp file.
    let inputs_id = uuid::Uuid::new_v4();
    let inputs_path = std::env::temp_dir().join(format!("forager-inputs-{inputs_id}.json"));
    let out_path = std::env::temp_dir().join(format!("forager-out-{inputs_id}.json"));

    std::fs::write(&inputs_path, serde_json::to_string(inputs)?)
        .context("writing FORAGER_INPUTS file")?;

    let status = Command::new(&binary)
        .env("FORAGER_INPUTS", &inputs_path)
        .env("FORAGER_OUT", &out_path)
        .env("FORAGER_STEP", step_name)
        .current_dir(project_dir)
        .status()
        .with_context(|| format!("spawning {binary_name}"))?;

    if !status.success() {
        log::warn!(
            "step '{}': {binary_name} exited with {status} — skipping measurement",
            step_name,
        );
        return Ok(None);
    }

    let envelope_raw = match std::fs::read_to_string(&out_path) {
        Ok(s) => s,
        Err(_) => {
            log::warn!("step '{}': {binary_name} did not write FORAGER_OUT", step_name);
            return Ok(None);
        }
    };

    let envelope: ForagerPluginEnvelope = serde_json::from_str(&envelope_raw)
        .with_context(|| format!("parsing output from {binary_name}"))?;

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
        /// Benchmark name (matches .wezel/benchmarks/<name>/).
        #[arg(short, long)]
        benchmark: String,
        /// Project root directory (defaults to current directory).
        #[arg(long)]
        project_dir: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    env_logger::init();
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Run {
            benchmark,
            project_dir,
        } => run_benchmark(&benchmark, project_dir.as_deref()),
    }
}

fn run_benchmark(benchmark_name: &str, project_dir_arg: Option<&Path>) -> Result<()> {
    let project_dir = match project_dir_arg {
        Some(p) => p.to_path_buf(),
        None => std::env::current_dir().context("getting current directory")?,
    };

    let config = load_config(&project_dir)?;
    let benchmark_dir = project_dir
        .join(".wezel")
        .join("benchmarks")
        .join(benchmark_name);

    if !benchmark_dir.is_dir() {
        bail!(
            "benchmark directory not found: {}",
            benchmark_dir.display()
        );
    }

    let (_name, _description, steps) = parse_benchmark(&benchmark_dir)?;

    // Detect current commit info from git.
    let commit_sha = git_current_sha(&project_dir)?;
    let project_upstream = git_upstream(&project_dir)?;
    let commit_author = git_commit_author(&project_dir);
    let commit_message = git_commit_message(&project_dir);
    let commit_timestamp = git_commit_timestamp(&project_dir);

    log::info!(
        "claiming job: upstream={} sha={} benchmark={}",
        project_upstream,
        &commit_sha[..7.min(commit_sha.len())],
        benchmark_name
    );

    // Claim the job from Burrow.
    let claim_body = serde_json::json!({
        "project_upstream": project_upstream,
        "commit_sha": commit_sha,
        "benchmark_name": benchmark_name,
        "commit_author": commit_author,
        "commit_message": commit_message,
        "commit_timestamp": commit_timestamp,
    });

    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(30))
        .build();

    let job: ForagerJob = agent
        .post(&format!("{}/api/forager/claim", config.server_url))
        .send_json(&claim_body)
        .context("claiming job from Burrow")?
        .into_json()
        .context("parsing claim response")?;

    log::info!("job claimed (token: {})", &job.token[..8.min(job.token.len())]);

    // Run each step.
    let mut step_reports: Vec<ForagerStepReport> = Vec::new();

    for step in &steps {
        log::info!("step '{}' [forager={}]", step.name, step.forager);

        // Apply patch if one exists.
        let patch_stem = step.diff.as_deref().unwrap_or(&step.name);
        let patch_path = benchmark_dir.join(format!("{patch_stem}.patch"));
        if patch_path.is_file() {
            log::info!("  applying patch: {}", patch_path.display());
            git_apply_patch(&project_dir, &patch_path)
                .with_context(|| format!("applying patch for step '{}'", step.name))?;
        }

        // Invoke the forager plugin.
        let measurement = invoke_forager(
            &step.forager,
            &step.name,
            &step.inputs,
            &project_dir,
        );

        match measurement {
            Ok(m) => {
                step_reports.push(ForagerStepReport {
                    step: step.name.clone(),
                    measurement: m,
                });
            }
            Err(e) => {
                log::warn!("step '{}' failed: {e:#}", step.name);
                step_reports.push(ForagerStepReport {
                    step: step.name.clone(),
                    measurement: None,
                });
            }
        }
    }

    // Submit results to Burrow.
    let report = ForagerRunReport {
        token: job.token.clone(),
        steps: step_reports,
    };

    agent
        .post(&format!("{}/api/forager/run", config.server_url))
        .send_json(&report)
        .context("submitting run report to Burrow")?;

    log::info!("run complete — results submitted");
    println!("Run complete.");

    Ok(())
}
