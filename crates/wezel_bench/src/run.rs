use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::Serialize;
use wezel_types::{ForagerRunReport, ForagerStepReport, SummaryDef};

use crate::git;
use crate::workspace::Scratch;
use crate::{Config, ExperimentToml, Workspace, fetch, invoke_forager, parse_experiment};

/// JSON output for `wezel experiment run --output-format json`.
#[derive(Debug, Serialize)]
pub struct ExperimentRunOutput {
    pub experiment: String,
    pub commit: String,
    pub steps: Vec<ForagerStepReport>,
    pub summaries: HashMap<String, SummaryValue>,
}

#[derive(Debug, Serialize)]
pub struct SummaryValue {
    pub value: f64,
    pub bisect: bool,
}

/// Compute summary values from step reports using the experiment's summary definitions.
///
/// Summaries that fail to compute (e.g. ambiguous aggregation) are logged at
/// warn level and omitted from the result.
pub fn compute_summaries(
    step_reports: &[ForagerStepReport],
    summary_defs: &[SummaryDef],
) -> HashMap<String, SummaryValue> {
    let mut result = HashMap::new();
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

    // Isolate the run: fresh clone of the user's repo at `commit_sha`, into
    // a tempdir that's removed when `scratch` drops. Foragers run inside
    // this scratch checkout so `target/` and step patches never touch the
    // user's working tree.
    let scratch = Scratch::create(&workspace.project_dir, &commit_sha)?;
    log::debug!("scratch checkout at {}", scratch.path().display());
    let scratch_workspace = Workspace {
        project_dir: scratch.path().to_path_buf(),
        plugin_dir: workspace.plugin_dir.clone(),
        config: Config::load(scratch.path())?,
    };

    // Run each step.
    let mut step_reports: Vec<ForagerStepReport> = Vec::new();

    for step in &experiment.steps {
        log::info!("step '{}' [forager={}]", step.name, step.forager);

        // Apply patch if the step declares one. Patch files come from the
        // user's experiment dir; they're applied inside the scratch checkout.
        if let Some(ref patch_stem) = step.diff {
            let patch_path = experiment_dir.join(format!("{patch_stem}.patch"));
            log::info!("  applying patch: {}", patch_path.display());
            git::apply_patch(&scratch_workspace.project_dir, &patch_path)
                .with_context(|| format!("applying patch for step '{}'", step.name))?;
        }

        // Invoke the forager plugin.
        let result = invoke_forager(
            &step.forager,
            &step.name,
            &step.inputs,
            &scratch_workspace,
            fetcher.as_deref_mut(),
        );

        match result {
            Ok(measurements) => {
                step_reports.push(ForagerStepReport {
                    step: step.name.clone(),
                    measurements,
                });
            }
            Err(e) if e.is_hard() => bail!("{e}"),
            Err(e) => {
                log::warn!("{e}");
                step_reports.push(ForagerStepReport {
                    step: step.name.clone(),
                    measurements: vec![],
                });
            }
        }
    }

    log::debug!(
        "experiment '{experiment_name}' finished at {}",
        &commit_sha[..7.min(commit_sha.len())]
    );

    Ok((step_reports, experiment.summaries))
}
