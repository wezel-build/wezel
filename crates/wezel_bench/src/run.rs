use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use wezel_types::{ExperimentRunStep, SummaryDef};

use crate::git;
use crate::workspace::{Scratch, Snapshot};
use crate::{ExperimentToml, ProjectConfig, Workspace, fetch, invoke_forager, parse_experiment};

/// One entry in the up-front plan handed to a `RunReporter` so it can size
/// progress UI before any step actually starts.
#[derive(Debug, Clone)]
pub struct StepPlan {
    pub name: String,
    /// Forager that runs the step; renders the TOML path `step.<forager>.<name>`.
    pub forager: String,
    pub samples: usize,
}

/// Receives lifecycle events during `run_experiment`. Default impls are noops
/// so renderers can override only what they need.
///
/// Pass `None` for headless callers and a real implementation (e.g.
/// indicatif-backed) from interactive CLI commands.
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

/// What one experiment run produced: the per-step outcomes, the summary
/// definitions they feed, and the plan as executed (step order, forager,
/// sample counts) for renderers that report per-step detail.
#[derive(Debug)]
pub struct CompletedRun {
    pub steps: Vec<ExperimentRunStep>,
    pub summaries: Vec<SummaryDef>,
    pub plan: Vec<StepPlan>,
}

/// JSON output for `wezel experiment run --output-format json`.
#[derive(Debug, Serialize, Deserialize)]
pub struct ExperimentRunOutput {
    pub experiment: String,
    pub commit: String,
    pub steps: Vec<ExperimentRunStep>,
    pub summaries: IndexMap<String, SummaryValue>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SummaryValue {
    pub value: f64,
    pub bisect: bool,
}

/// Current [`SavedRun::schema_version`]. Runs written by an older version are
/// skipped when looking for a comparison baseline.
pub const SAVED_RUN_SCHEMA_VERSION: u32 = 1;

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

/// A timestamp that isn't in the `YYYY-MM-DDTHH:MM:SSZ` form written by
/// [`utc_timestamp_rfc3339`].
#[derive(Debug, Clone, thiserror::Error)]
#[error("invalid UTC timestamp {input:?}, expected YYYY-MM-DDTHH:MM:SSZ")]
pub struct ParseTimestampError {
    pub input: String,
}

/// Unix seconds for a `YYYY-MM-DDTHH:MM:SSZ` timestamp. Only the exact shape
/// [`utc_timestamp_rfc3339`] emits is accepted — no offsets, no fractions.
pub fn unix_seconds(ts: &str) -> Result<i64, ParseTimestampError> {
    fn parse(ts: &str) -> Option<i64> {
        let (date, time) = ts.strip_suffix('Z')?.split_once('T')?;
        let mut date = date.split('-');
        let mut time = time.split(':');
        fn field(part: Option<&str>) -> Option<i64> {
            part?.parse::<i64>().ok()
        }
        let (y, mo, d) = (
            field(date.next())?,
            field(date.next())?,
            field(date.next())?,
        );
        let (h, mi, s) = (
            field(time.next())?,
            field(time.next())?,
            field(time.next())?,
        );
        if date.next().is_some() || time.next().is_some() {
            return None;
        }
        if !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
            return None;
        }
        if h > 23 || mi > 59 || s > 60 {
            return None;
        }
        Some(days_from_civil(y, mo, d) * 86_400 + h * 3_600 + mi * 60 + s)
    }

    parse(ts).ok_or_else(|| ParseTimestampError {
        input: ts.to_owned(),
    })
}

/// Days since 1970-01-01 in the proleptic Gregorian calendar (Hinnant's
/// `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Most recent saved run for `experiment`, used as the comparison baseline by
/// `wezel experiment run`. Run directories are named `<started_at>-<short sha>`,
/// so reverse lexicographic order is reverse chronological. Runs that can't be
/// read, or that predate [`SAVED_RUN_SCHEMA_VERSION`], are skipped.
///
/// Call this *before* the current run is saved, or it will find that run.
pub fn load_previous_run(workspace: &crate::Workspace, experiment: &str) -> Option<SavedRun> {
    let exp_dir = workspace
        .project_dir
        .join(".wezel")
        .join("runs")
        .join(experiment);

    let mut ids: Vec<_> = std::fs::read_dir(&exp_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name())
        .collect();
    ids.sort();

    for id in ids.iter().rev() {
        let path = exp_dir.join(id).join("run.json");
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        match serde_json::from_slice::<SavedRun>(&bytes) {
            Ok(run) if run.schema_version == SAVED_RUN_SCHEMA_VERSION => return Some(run),
            Ok(run) => log::debug!(
                "baseline: skipping {} (schema_version {})",
                path.display(),
                run.schema_version
            ),
            Err(e) => log::debug!("baseline: skipping {}: {e}", path.display()),
        }
    }
    None
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
    step_reports: &[ExperimentRunStep],
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

/// Run an experiment against the current checkout and return the step reports,
/// conclusion definitions, and executed plan.
///
/// Clones the project at `HEAD` into scratch, overlaying uncommitted
/// working-tree edits, so the measured tree matches what the user is looking
/// at. The experiment definition is read from that clone.
///
/// This function is pure execution — it knows nothing about Burrow. The caller
/// decides whether/how to submit results.
pub fn run_experiment(
    experiment_name: &str,
    workspace: &Workspace,
    fetcher: Option<&mut (dyn fetch::PluginFetcher + '_)>,
    reporter: Option<&dyn RunReporter>,
) -> Result<CompletedRun> {
    let commit_sha = git::current_sha(&workspace.project_dir)?;
    let scratch = Scratch::create_with_worktree(&workspace.project_dir, &commit_sha)?;
    run_in_scratch(
        experiment_name,
        scratch,
        &commit_sha,
        &workspace.tool_store,
        fetcher,
        reporter,
    )
}

/// Run an experiment at a specific committed `sha`, cloning the repo from
/// `repo_src`. Used by the run queue (`wezel experiment next`).
///
/// Unlike [`run_experiment`], the working tree is never consulted: both the
/// code and the experiment definition come from `sha`'s committed tree, so a
/// bisection across commits where the definition changed measures the right
/// definition at each one. If `sha` isn't present in `repo_src`, it is fetched
/// from `origin` first.
pub fn run_experiment_at(
    experiment_name: &str,
    repo_src: &Path,
    sha: &str,
    tool_store: &Path,
    fetcher: Option<&mut (dyn fetch::PluginFetcher + '_)>,
    reporter: Option<&dyn RunReporter>,
) -> Result<CompletedRun> {
    git::ensure_commit(repo_src, sha)?;
    let scratch = Scratch::create(repo_src, sha)?;
    run_in_scratch(experiment_name, scratch, sha, tool_store, fetcher, reporter)
}

/// Shared execution engine: measure `experiment_name` inside an already-prepared
/// `scratch` clone checked out at `commit_sha`. The experiment definition and
/// any step patches are read from the clone. Foragers run inside the clone, so
/// `target/` and step patches never touch the caller's working tree.
fn run_in_scratch(
    experiment_name: &str,
    scratch: Scratch,
    commit_sha: &str,
    tool_store: &Path,
    mut fetcher: Option<&mut (dyn fetch::PluginFetcher + '_)>,
    reporter: Option<&dyn RunReporter>,
) -> Result<CompletedRun> {
    log::debug!("scratch checkout at {}", scratch.path().display());
    let project_dir = scratch.project_dir();
    let scratch_workspace = Workspace {
        project_dir: project_dir.clone(),
        tool_store: tool_store.to_path_buf(),
        config: ProjectConfig::load(&project_dir)?,
    };

    let experiment_dir = scratch_workspace
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
            forager: s.forager.clone(),
            samples: step_samples
                .get(s.name.as_str())
                .copied()
                .unwrap_or(1)
                .max(1),
        })
        .collect();
    if let Some(r) = reporter {
        r.run_started(experiment_name, commit_sha, &plan);
    }

    // Run each step.
    let mut step_reports: Vec<ExperimentRunStep> = Vec::new();

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
            git::apply_patch(scratch.path(), &patch_path)
                .with_context(|| format!("applying patch for step '{}'", step.name))?;
        }

        // Take a snapshot once when sampling — every iteration restores from
        // it, making them i.i.d. The post-state of the last iter is what
        // downstream steps see.
        let snapshot = (samples > 1)
            .then(|| Snapshot::capture(scratch.path()))
            .transpose()
            .with_context(|| format!("snapshotting before step '{}'", step.name))?;

        let mut all_measurements = Vec::new();
        let mut hard_failure = None;
        for iter in 1..=samples {
            if iter > 1
                && let Some(ref snap) = snapshot
            {
                snap.restore_to(scratch.path()).with_context(|| {
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

        step_reports.push(ExperimentRunStep {
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

    Ok(CompletedRun {
        steps: step_reports,
        summaries: experiment.summaries,
        plan,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_seconds_matches_known_epochs() {
        assert_eq!(unix_seconds("1970-01-01T00:00:00Z").unwrap(), 0);
        assert_eq!(unix_seconds("2026-08-15T12:34:56Z").unwrap(), 1_786_797_296);
        // Leap day, so the civil-date conversion has to handle it.
        assert_eq!(unix_seconds("2024-02-29T00:00:00Z").unwrap(), 1_709_164_800);
    }

    #[test]
    fn unix_seconds_rejects_other_shapes() {
        for bad in [
            "unknown",
            "2026-08-15T12:34:56+02:00",
            "2026-08-15T12:34:56.123Z",
            "2026-08-15Z",
            "2026-13-01T00:00:00Z",
            "2026-08-15T24:00:00Z",
        ] {
            assert!(unix_seconds(bad).is_err(), "{bad} should not parse");
        }
    }

    /// Write a `run.json` under `<project>/.wezel/runs/<exp>/<id>/`.
    fn write_run(project: &Path, experiment: &str, id: &str, schema_version: u32, commit: &str) {
        let dir = project
            .join(".wezel")
            .join("runs")
            .join(experiment)
            .join(id);
        std::fs::create_dir_all(&dir).unwrap();
        let run = serde_json::json!({
            "schema_version": schema_version,
            "wezel_version": "0.0.0",
            "started_at": "2026-08-09T12:00:00Z",
            "duration_ms": 1000,
            "dirty": false,
            "branch": "main",
            "output": {
                "experiment": experiment,
                "commit": commit,
                "steps": [],
                "summaries": {},
            },
        });
        std::fs::write(dir.join("run.json"), run.to_string()).unwrap();
    }

    fn workspace_at(project_dir: &Path) -> Workspace {
        Workspace {
            project_dir: project_dir.to_path_buf(),
            tool_store: project_dir.join("tools"),
            config: ProjectConfig {
                project_id: uuid::Uuid::nil(),
                name: "test".into(),
                tools: Default::default(),
            },
        }
    }

    #[test]
    fn previous_run_is_the_newest_readable_one() {
        let tmp = tempfile::tempdir().unwrap();
        write_run(
            tmp.path(),
            "build",
            "2026-08-01T00-00-00Z-aaaaaaa",
            1,
            "aaaaaaa",
        );
        write_run(
            tmp.path(),
            "build",
            "2026-08-09T00-00-00Z-bbbbbbb",
            1,
            "bbbbbbb",
        );
        let ws = workspace_at(tmp.path());
        let previous = load_previous_run(&ws, "build").unwrap();
        assert_eq!(previous.output.commit, "bbbbbbb");
    }

    #[test]
    fn previous_run_skips_other_schema_versions() {
        let tmp = tempfile::tempdir().unwrap();
        write_run(
            tmp.path(),
            "build",
            "2026-08-01T00-00-00Z-aaaaaaa",
            1,
            "aaaaaaa",
        );
        write_run(
            tmp.path(),
            "build",
            "2026-08-09T00-00-00Z-bbbbbbb",
            99,
            "bbbbbbb",
        );
        let ws = workspace_at(tmp.path());
        let previous = load_previous_run(&ws, "build").unwrap();
        assert_eq!(previous.output.commit, "aaaaaaa");
    }

    #[test]
    fn previous_run_is_absent_for_a_first_run() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = workspace_at(tmp.path());
        assert!(load_previous_run(&ws, "build").is_none());
    }

    #[test]
    fn day_difference_is_whole_days() {
        let a = unix_seconds("2026-08-09T10:00:00Z").unwrap();
        let b = unix_seconds("2026-08-15T10:00:00Z").unwrap();
        assert_eq!((b - a) / 86_400, 6);
    }
}
