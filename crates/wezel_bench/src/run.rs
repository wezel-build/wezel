use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use wezel_types::{ForagerRunReport, ForagerStepReport, SummaryDef};

use crate::git;
use crate::workspace::{Scratch, Snapshot};
use crate::{ExperimentToml, ProjectConfig, Workspace, fetch, invoke_forager, parse_experiment};

/// One entry in the up-front plan handed to a `RunReporter` so it can size
/// progress UI before any step actually starts.
#[derive(Debug, Clone)]
pub struct StepPlan {
    pub name: String,
    pub samples: usize,
}

/// Receives lifecycle events during `run_experiment`. Default impls are noops
/// so renderers can override only what they need.
///
/// Pass `None` for headless callers (daemon, standalone bisect) and a real
/// implementation (e.g. indicatif-backed) from interactive CLI commands.
pub trait RunReporter: Send + Sync {
    fn run_started(&self, _experiment: &str, _commit: &str, _steps: &[StepPlan]) {}
    fn step_started(&self, _step: &str) {}
    /// Forager invocation is about to start. Paired with `sample_done`. Use
    /// these brackets to measure forager-only time (excluding snapshot copy /
    /// restore between samples).
    fn sample_started(&self, _step: &str, _iter: usize, _samples: usize) {}
    fn sample_done(&self, _step: &str, _iter: usize, _samples: usize) {}
    fn step_finished(&self, _step: &str) {}
    fn run_finished(&self) {}
}

/// JSON output for `wezel experiment run --output-format json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExperimentRunOutput {
    pub experiment: String,
    pub commit: String,
    pub steps: Vec<ForagerStepReport>,
    pub summaries: IndexMap<String, SummaryValue>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SummaryValue {
    pub value: f64,
    pub bisect: bool,
}

/// On-disk record of one `wezel experiment run` invocation, written to
/// `.wezel/runs/<experiment>/<id>/run.json`. Bump `schema_version` whenever
/// the shape changes incompatibly so older runs can be detected and skipped.
#[derive(Debug, Serialize, Deserialize)]
pub struct SavedRun {
    pub schema_version: u32,
    pub wezel_version: String,
    /// RFC3339 UTC timestamp captured immediately before `run_experiment` started.
    pub started_at: String,
    pub duration_ms: u64,
    /// Whether tracked files were modified at the time the run started. The
    /// run itself measures HEAD via a scratch clone, so this is informational —
    /// it tells you the user's tree didn't match the commit that was measured.
    pub dirty: bool,
    /// Branch HEAD pointed at, or `None` when detached.
    pub branch: Option<String>,
    pub output: ExperimentRunOutput,
}

/// RFC3339 UTC timestamp using the `date` command — matches the chrono-free
/// approach in `daemon.rs`. Returns `"unknown"` if `date` is unavailable.
pub fn utc_timestamp_rfc3339() -> String {
    std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

/// Persist a run under `.wezel/runs/<experiment>/<id>/run.json` and return the
/// run directory. Creates `.wezel/runs/.gitignore` on first use so saved runs
/// never get committed.
pub fn save_run(workspace: &crate::Workspace, run: &SavedRun) -> Result<std::path::PathBuf> {
    let runs_root = workspace.project_dir.join(".wezel").join("runs");
    std::fs::create_dir_all(&runs_root)
        .with_context(|| format!("creating {}", runs_root.display()))?;

    // Self-ignoring gitignore: `*` includes the .gitignore itself, so git
    // never reports anything under `.wezel/runs/` as untracked.
    let gi = runs_root.join(".gitignore");
    if !gi.exists() {
        std::fs::write(&gi, "*\n").with_context(|| format!("writing {}", gi.display()))?;
    }

    let exp_dir = runs_root.join(&run.output.experiment);
    std::fs::create_dir_all(&exp_dir).with_context(|| format!("creating {}", exp_dir.display()))?;

    let short = &run.output.commit[..7.min(run.output.commit.len())];
    let id = format!("{}-{}", run.started_at.replace(':', "-"), short);

    // Collision guard for same-second runs against the same commit.
    let mut run_dir = exp_dir.join(&id);
    let mut suffix = 1;
    while run_dir.exists() {
        run_dir = exp_dir.join(format!("{id}-{suffix}"));
        suffix += 1;
    }
    std::fs::create_dir_all(&run_dir).with_context(|| format!("creating {}", run_dir.display()))?;

    let run_json = run_dir.join("run.json");
    let bytes = serde_json::to_vec_pretty(run).context("serializing SavedRun")?;
    std::fs::write(&run_json, bytes).with_context(|| format!("writing {}", run_json.display()))?;
    Ok(run_dir)
}

/// Compute summary values from step reports using the experiment's summary definitions.
///
/// Summaries that fail to compute (e.g. ambiguous aggregation) are logged at
/// warn level and omitted from the result.
pub fn compute_summaries(
    step_reports: &[ForagerStepReport],
    summary_defs: &[SummaryDef],
) -> IndexMap<String, SummaryValue> {
    let mut result = IndexMap::new();
    for def in summary_defs {
        match def.compute(step_reports) {
            Ok(Some(value)) => {
                result.insert(
                    def.name.clone(),
                    SummaryValue {
                        value,
                        bisect: def.bisect,
                    },
                );
            }
            Ok(None) => {}
            Err(e) => {
                log::warn!("summary '{}' skipped: {e}", def.name);
            }
        }
    }
    result
}

pub struct BurrowSession {
    agent: ureq::Agent,
    server_url: String,
}

impl BurrowSession {
    pub fn new(server_url: &str) -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .build(),
            server_url: server_url.to_string(),
        }
    }

    pub fn submit(&self, report: &ForagerRunReport) -> Result<()> {
        self.agent
            .post(&format!("{}/api/forager/run", self.server_url))
            .send_json(report)
            .context("submitting run report to Burrow")?;
        Ok(())
    }
}

pub fn list_experiments(project_dir: &Path) -> Result<()> {
    let experiments_dir = project_dir.join(".wezel").join("experiments");
    if !experiments_dir.is_dir() {
        bail!("no experiments directory at {}", experiments_dir.display());
    }

    let mut found = Vec::new();
    for entry in std::fs::read_dir(&experiments_dir).context("reading experiments directory")? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir()
            && path.join("experiment.toml").is_file()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
        {
            let toml_path = path.join("experiment.toml");
            let description = std::fs::read_to_string(&toml_path)
                .ok()
                .and_then(|raw| toml::from_str::<ExperimentToml>(&raw).ok())
                .and_then(|b| b.description);
            found.push((name.to_string(), description));
        }
    }

    if found.is_empty() {
        println!("No experiments found in {}", experiments_dir.display());
        return Ok(());
    }

    found.sort_by(|a, b| a.0.cmp(&b.0));
    println!("Available experiments:\n");
    for (name, desc) in &found {
        match desc {
            Some(d) => println!("  {name}  — {d}"),
            None => println!("  {name}"),
        }
    }
    println!("\nRun with: wezel experiment run -e <name>");

    Ok(())
}

/// Run an experiment and return the step reports plus conclusion definitions.
///
/// This function is pure execution — it knows nothing about Burrow.  The
/// caller (daemon or CLI) decides whether/how to submit results.
pub fn run_experiment(
    experiment_name: &str,
    workspace: &Workspace,
    mut fetcher: Option<&mut (dyn fetch::PluginFetcher + '_)>,
    reporter: Option<&dyn RunReporter>,
) -> Result<(Vec<ForagerStepReport>, Vec<SummaryDef>)> {
    let experiment_dir = workspace
        .project_dir
        .join(".wezel")
        .join("experiments")
        .join(experiment_name);

    if !experiment_dir.is_dir() {
        bail!(
            "experiment directory not found: {}",
            experiment_dir.display()
        );
    }

    let experiment = parse_experiment(&experiment_dir)?;
    let commit_sha = git::current_sha(&workspace.project_dir)?;

    // Per-step sample count is derived from summaries; lint enforces a single
    // value per step, so taking max here just guards against a stale lockfile
    // where lint hasn't been re-run.
    let mut step_samples: HashMap<&str, usize> = HashMap::new();
    for summary in &experiment.summaries {
        let entry = step_samples.entry(summary.step.as_str()).or_insert(1);
        *entry = (*entry).max(summary.samples);
    }

    let plan: Vec<StepPlan> = experiment
        .steps
        .iter()
        .map(|s| StepPlan {
            name: s.name.clone(),
            samples: step_samples
                .get(s.name.as_str())
                .copied()
                .unwrap_or(1)
                .max(1),
        })
        .collect();
    if let Some(r) = reporter {
        r.run_started(experiment_name, &commit_sha, &plan);
    }

    // Isolate the run: fresh clone of the user's repo at `commit_sha`, into
    // a tempdir that's removed when `scratch` drops. Foragers run inside
    // this scratch checkout so `target/` and step patches never touch the
    // user's working tree.
    let scratch = Scratch::create(&workspace.project_dir, &commit_sha)?;
    log::debug!("scratch checkout at {}", scratch.path().display());
    let scratch_workspace = Workspace {
        project_dir: scratch.path().to_path_buf(),
        plugin_dir: workspace.plugin_dir.clone(),
        config: ProjectConfig::load(scratch.path())?,
    };

    // Run each step.
    let mut step_reports: Vec<ForagerStepReport> = Vec::new();

    for step in &experiment.steps {
        let samples = step_samples
            .get(step.name.as_str())
            .copied()
            .unwrap_or(1)
            .max(1);
        log::info!(
            "step '{}' [forager={}, samples={samples}]",
            step.name,
            step.forager
        );
        if let Some(r) = reporter {
            r.step_started(&step.name);
        }

        // Apply patch if the step declares one. Patch files come from the
        // user's experiment dir; they're applied inside the scratch checkout.
        if let Some(ref patch_stem) = step.diff {
            let patch_path = experiment_dir.join(format!("{patch_stem}.patch"));
            log::info!("  applying patch: {}", patch_path.display());
            git::apply_patch(&scratch_workspace.project_dir, &patch_path)
                .with_context(|| format!("applying patch for step '{}'", step.name))?;
        }

        // Take a snapshot once when sampling — every iteration restores from
        // it, making them i.i.d. The post-state of the last iter is what
        // downstream steps see.
        let snapshot = (samples > 1)
            .then(|| Snapshot::capture(&scratch_workspace.project_dir))
            .transpose()
            .with_context(|| format!("snapshotting before step '{}'", step.name))?;

        let mut all_measurements = Vec::new();
        let mut hard_failure = None;
        for iter in 1..=samples {
            if iter > 1
                && let Some(ref snap) = snapshot
            {
                snap.restore_to(&scratch_workspace.project_dir)
                    .with_context(|| {
                        format!("restoring snapshot for step '{}' iter {iter}", step.name)
                    })?;
            }
            log::debug!("  iter {iter}/{samples}");
            if let Some(r) = reporter {
                r.sample_started(&step.name, iter, samples);
            }
            match invoke_forager(
                &step.forager,
                &step.name,
                &step.inputs,
                &scratch_workspace,
                fetcher.as_deref_mut(),
            ) {
                Ok(mut measurements) => all_measurements.append(&mut measurements),
                Err(e) if e.is_hard() => {
                    hard_failure = Some(e);
                    break;
                }
                Err(e) => log::warn!("{e}"),
            }
            if let Some(r) = reporter {
                r.sample_done(&step.name, iter, samples);
            }
        }

        if let Some(e) = hard_failure {
            bail!("{e}");
        }

        if let Some(r) = reporter {
            r.step_finished(&step.name);
        }

        step_reports.push(ForagerStepReport {
            step: step.name.clone(),
            measurements: all_measurements,
        });
    }

    if let Some(r) = reporter {
        r.run_finished();
    }

    log::debug!(
        "experiment '{experiment_name}' finished at {}",
        &commit_sha[..7.min(commit_sha.len())]
    );

    Ok((step_reports, experiment.summaries))
}
