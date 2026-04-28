use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize;

use crate::{fetch, parse_experiment, resolve_plugin};

struct LintDiagnostic {
    step: String,
    message: String,
}

struct ExperimentResult {
    name: String,
    step_count: usize,
    diagnostics: Vec<LintDiagnostic>,
}

pub fn run_lint(
    project_dir: &Path,
    mut fetcher: Option<&mut (dyn fetch::PluginFetcher + '_)>,
) -> Result<()> {
    let experiments_dir = project_dir.join(".wezel").join("experiments");
    if !experiments_dir.is_dir() {
        bail!("no experiments directory at {}", experiments_dir.display());
    }

    let mut dirs: Vec<_> = std::fs::read_dir(&experiments_dir)
        .context("reading experiments directory")?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir() && e.path().join("experiment.toml").is_file())
        .collect();
    dirs.sort_by_key(|e| e.file_name());

    if dirs.is_empty() {
        bail!("no experiments found in {}", experiments_dir.display());
    }

    let mut results: Vec<ExperimentResult> = Vec::new();
    let mut warned_plugins: HashSet<String> = HashSet::new();

    for entry in &dirs {
        let experiment_dir = entry.path();
        let experiment_name = entry.file_name().to_string_lossy().to_string();

        // Parse the TOML.
        let steps = match parse_experiment(&experiment_dir) {
            Ok(exp) => exp.steps,
            Err(e) => {
                results.push(ExperimentResult {
                    name: experiment_name,
                    step_count: 0,
                    diagnostics: vec![LintDiagnostic {
                        step: String::new(),
                        message: format!("failed to parse: {e}"),
                    }],
                });
                continue;
            }
        };

        let mut diagnostics = Vec::new();

        for step in &steps {
            // Check patch file exists when declared.
            if let Some(ref patch_stem) = step.diff {
                let patch_path = experiment_dir.join(format!("{patch_stem}.patch"));
                if !patch_path.is_file() {
                    diagnostics.push(LintDiagnostic {
                        step: step.name.clone(),
                        message: format!("{patch_stem}.patch not found"),
                    });
                }
            }

            // Check plugin is in the local store; try fetching if a fetcher is available.
            if resolve_plugin(&step.forager).is_none() {
                if let Some(ref mut f) = fetcher {
                    match f.fetch(&step.forager) {
                        Ok(_) => {} // installed, proceed to schema check
                        Err(e) => {
                            if warned_plugins.insert(step.forager.clone()) {
                                diagnostics.push(LintDiagnostic {
                                    step: step.name.clone(),
                                    message: format!("plugin `forager-{}`: {e}", step.forager),
                                });
                            }
                        }
                    }
                } else if warned_plugins.insert(step.forager.clone()) {
                    diagnostics.push(LintDiagnostic {
                        step: step.name.clone(),
                        message: format!("plugin `forager-{}` not in local store", step.forager),
                    });
                }
            }

            // If plugin is available, validate its --schema output.
            if let Some(binary) = resolve_plugin(&step.forager) {
                match Command::new(&binary).arg("--schema").output() {
                    Ok(o) if o.status.success() => {
                        let stdout = String::from_utf8_lossy(&o.stdout);
                        if serde_json::from_str::<serde_json::Value>(&stdout).is_err() {
                            diagnostics.push(LintDiagnostic {
                                step: step.name.clone(),
                                message: format!(
                                    "`forager-{} --schema` returned invalid JSON",
                                    step.forager
                                ),
                            });
                        }
                    }
                    Ok(o) => {
                        diagnostics.push(LintDiagnostic {
                            step: step.name.clone(),
                            message: format!(
                                "`forager-{} --schema` exited with {}",
                                step.forager, o.status
                            ),
                        });
                    }
                    Err(e) => {
                        diagnostics.push(LintDiagnostic {
                            step: step.name.clone(),
                            message: format!(
                                "failed to run `forager-{} --schema`: {e}",
                                step.forager
                            ),
                        });
                    }
                }
            }
        }

        results.push(ExperimentResult {
            name: experiment_name,
            step_count: steps.len(),
            diagnostics,
        });
    }

    // Render output.
    let total_errors: usize = results.iter().map(|r| r.diagnostics.len()).sum();
    let total_experiments = results.len();

    for result in &results {
        let ok = result.diagnostics.is_empty() && result.step_count > 0;
        let steps_label = format!(
            "{} step{}",
            result.step_count,
            if result.step_count == 1 { "" } else { "s" }
        );

        if ok {
            println!(
                "  {} {} {}",
                result.name.bold(),
                steps_label.dimmed(),
                "ok".green().bold(),
            );
        } else {
            println!(
                "  {} {} {}",
                result.name.bold(),
                steps_label.dimmed(),
                "FAIL".red().bold(),
            );
            for d in &result.diagnostics {
                if d.step.is_empty() {
                    eprintln!("    {} {}", "-".red(), d.message);
                } else {
                    eprintln!("    {} {}: {}", "-".red(), d.step.dimmed(), d.message,);
                }
            }
        }
    }

    println!();
    if total_errors == 0 {
        println!(
            "{}",
            format!(
                "{total_experiments} experiment{} validated, no errors.",
                if total_experiments == 1 { "" } else { "s" }
            )
            .green()
        );
        Ok(())
    } else {
        let msg = format!(
            "{total_experiments} experiment{} checked, {total_errors} error{} found.",
            if total_experiments == 1 { "" } else { "s" },
            if total_errors == 1 { "" } else { "s" },
        );
        eprintln!("{}", msg.red());
        bail!("{msg}");
    }
}
