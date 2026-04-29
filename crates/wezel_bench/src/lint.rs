use std::collections::{BTreeSet, HashMap, HashSet};

use anyhow::{Context, Result, bail};
use owo_colors::OwoColorize;

use crate::{Workspace, fetch, lockfile, parse_experiment};

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
    workspace: &Workspace,
    mut fetcher: Option<&mut (dyn fetch::PluginFetcher + '_)>,
) -> Result<()> {
    let experiments_dir = workspace.project_dir.join(".wezel").join("experiments");
    if !experiments_dir.is_dir() {
        bail!("no experiments directory at {}", experiments_dir.display());
    }

    // The lockfile is the source of truth for resolution; missing it is a
    // hard error so CI surfaces drift instead of silently fetching latest.
    let lock_path = lockfile::path(&workspace.project_dir);
    if !lock_path.is_file() {
        bail!(
            "no lockfile at {} — run `wezel experiment run` once to populate it",
            lock_path.display()
        );
    }
    let lock = lockfile::load(&workspace.project_dir)?;

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
        let (steps, summaries) = match parse_experiment(&experiment_dir) {
            Ok(exp) => (exp.steps, exp.summaries),
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

        // Summaries on the same step must agree on `samples` — the runner
        // takes one snapshot per step, so a divergent count would be
        // ambiguous.
        let mut samples_by_step: HashMap<&str, BTreeSet<usize>> = HashMap::new();
        for summary in &summaries {
            samples_by_step
                .entry(summary.step.as_str())
                .or_default()
                .insert(summary.samples);
        }
        for (step, counts) in &samples_by_step {
            if counts.len() > 1 {
                let rendered = counts
                    .iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                diagnostics.push(LintDiagnostic {
                    step: (*step).to_string(),
                    message: format!(
                        "summaries disagree on `samples` ({rendered}); all summaries on a step must use the same value"
                    ),
                });
            }
        }

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

            // The forager must be declared in [tools.foragers.<name>].
            // Skip downstream checks if it isn't — they'd just produce noise
            // about the same root cause.
            if !workspace.config.tools.foragers.contains_key(&step.forager) {
                if warned_plugins.insert(step.forager.clone()) {
                    diagnostics.push(LintDiagnostic {
                        step: step.name.clone(),
                        message: format!(
                            "forager `{}` is not declared — add `[tools.foragers.{}]` to .wezel/config.toml",
                            step.forager, step.forager
                        ),
                    });
                }
                continue;
            }

            // The lockfile must contain a locked entry; otherwise lint refuses
            // to fetch latest behind the user's back.
            if !lock.tools.foragers.contains_key(&step.forager) {
                if warned_plugins.insert(step.forager.clone()) {
                    diagnostics.push(LintDiagnostic {
                        step: step.name.clone(),
                        message: format!(
                            "forager `{}` is declared but not locked — run `wezel experiment run` to refresh wezel.lock",
                            step.forager
                        ),
                    });
                }
                continue;
            }

            // Install from the locked tag if missing locally. The fetcher
            // passed by lint runs in read-only mode so wezel.lock isn't
            // mutated.
            if workspace.resolve_plugin(&step.forager).is_none()
                && let Some(ref mut f) = fetcher
                && let Err(e) = f.fetch(&step.forager)
                && warned_plugins.insert(step.forager.clone())
            {
                diagnostics.push(LintDiagnostic {
                    step: step.name.clone(),
                    message: format!("plugin `forager-{}`: {e}", step.forager),
                });
                continue;
            }

            // Every declared forager must end up resolvable — its cached
            // schema is part of the contract that lint validates.
            if workspace.resolve_plugin(&step.forager).is_none() {
                if warned_plugins.insert(step.forager.clone()) {
                    diagnostics.push(LintDiagnostic {
                        step: step.name.clone(),
                        message: format!("plugin `forager-{}` not in local store", step.forager),
                    });
                }
                continue;
            }

            // Read the cached schema sidecar that the installer wrote. Lint
            // never invokes the binary for schema discovery.
            let schema_path = workspace.schema_path(&step.forager);
            match std::fs::read_to_string(&schema_path) {
                Ok(raw) => {
                    if serde_json::from_str::<serde_json::Value>(&raw).is_err() {
                        diagnostics.push(LintDiagnostic {
                            step: step.name.clone(),
                            message: format!(
                                "cached schema for `forager-{}` is invalid JSON ({})",
                                step.forager,
                                schema_path.display()
                            ),
                        });
                    }
                }
                Err(e) => {
                    diagnostics.push(LintDiagnostic {
                        step: step.name.clone(),
                        message: format!(
                            "no cached schema for `forager-{}` at {} ({e}) — reinstall the forager",
                            step.forager,
                            schema_path.display()
                        ),
                    });
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
