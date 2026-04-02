use std::path::Path;

use anyhow::{Context, Result, bail};
use wezel_types::{ForagerRunReport, ForagerStepReport};

use crate::git;
use crate::{ExperimentToml, Config, fetch, invoke_forager, parse_experiment};

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

/// Run an experiment and return the step reports.
///
/// This function is pure execution — it knows nothing about Burrow.  The
/// caller (daemon or CLI) decides whether/how to submit results.
pub fn run_experiment(
    experiment_name: &str,
    project_dir: &Path,
    fetcher: Option<&dyn fetch::PluginFetcher>,
) -> Result<Vec<ForagerStepReport>> {
    let experiment_dir = project_dir
        .join(".wezel")
        .join("experiments")
        .join(experiment_name);

    if !experiment_dir.is_dir() {
        bail!("experiment directory not found: {}", experiment_dir.display());
    }

    let (_name, _description, steps) = parse_experiment(&experiment_dir)?;

    let commit_sha = git::current_sha(project_dir)?;

    // Run each step.
    let mut step_reports: Vec<ForagerStepReport> = Vec::new();

    for step in &steps {
        log::info!("step '{}' [forager={}]", step.name, step.forager);

        // Apply patch if the step declares one.
        if let Some(ref patch_stem) = step.diff {
            let patch_path = experiment_dir.join(format!("{patch_stem}.patch"));
            log::info!("  applying patch: {}", patch_path.display());
            git::apply_patch(project_dir, &patch_path)
                .with_context(|| format!("applying patch for step '{}'", step.name))?;
        }

        // Invoke the forager plugin.
        let measurement = invoke_forager(&step.forager, &step.name, &step.inputs, project_dir, fetcher);

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
    println!("Experiment: {experiment_name}");
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

    Ok(step_reports)
}
