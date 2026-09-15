mod cmd;
mod config;
mod fetcher;
mod progress;
mod report;
mod report_artifacts;
mod style;

use anyhow::Context as _;

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::engine::{ArgValueCandidates, CompletionCandidate};
use std::path::PathBuf;
use std::process::ExitCode;

use cmd::init_cmd;

fn detect_upstream() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Some(normalize_upstream(&raw))
}

/// Strip protocol, user@, and .git suffix so SSH and HTTPS remotes match.
fn normalize_upstream(url: &str) -> String {
    let s = url
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("ssh://")
        .trim_start_matches("git://");
    // Handle git@host:user/repo style
    let s = if let Some(rest) = s.strip_prefix("git@") {
        rest.replacen(':', "/", 1)
    } else {
        s.to_string()
    };
    s.trim_end_matches(".git").to_string()
}

fn complete_experiments() -> Vec<CompletionCandidate> {
    let Ok(cwd) = std::env::current_dir() else {
        return vec![];
    };
    let experiments_dir = cwd.join(".wezel").join("experiments");
    let Ok(entries) = std::fs::read_dir(&experiments_dir) else {
        return vec![];
    };
    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir()
            && path.join("experiment.toml").is_file()
            && let Some(name) = path.file_name().and_then(|n| n.to_str())
        {
            let help = std::fs::read_to_string(path.join("experiment.toml"))
                .ok()
                .and_then(|raw| toml::from_str::<toml::Value>(&raw).ok())
                .and_then(|v| v.get("description")?.as_str().map(|s| s.to_string()));
            let mut c = CompletionCandidate::new(name);
            if let Some(h) = help {
                c = c.help(Some(h.into()));
            }
            candidates.push(c);
        }
    }
    candidates
}

#[derive(Parser)]
#[command(
    name = "wezel",
    about = "Build regression detection",
    version = concat!(env!("CARGO_PKG_VERSION"), " (", env!("WEZEL_BUILD_SHA"), ")"),
)]
struct Cli {
    /// Project root directory (defaults to current directory).
    #[arg(long, global = true, env = "WEZEL_PROJECT_DIR", value_name = "DIR")]
    project_dir: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Project-scoped commands: initialize and manage external tools.
    #[command(visible_alias = "p")]
    Project {
        #[command(subcommand)]
        cmd: ProjectCmd,
    },
    /// Active measurement: run experiments across commits.
    #[command(visible_alias = "exp", visible_alias = "e")]
    Experiment {
        #[command(subcommand)]
        cmd: ExperimentCmd,
    },
    /// Show how to enable dynamic shell completions.
    Completions,
}

#[derive(Subcommand)]
enum ProjectCmd {
    /// Initialize wezel in the current project.
    ///
    /// Creates `.wezel/config.toml` in the current directory.
    /// Options not passed on the command line are prompted interactively.
    Init {
        /// Fiflok API URL to push build timings to.
        #[arg(long)]
        server_url: Option<String>,
    },
    /// Manage external tools declared under `[tools]` in `.wezel/config.toml`.
    Tool {
        #[command(subcommand)]
        cmd: ToolCmd,
    },
    /// Show the resolved project config, declared tools, and lockfile state.
    Status,
}

#[derive(Subcommand)]
enum ToolCmd {
    /// Install every declared tool to the local store and refresh `wezel.lock`.
    ///
    /// Idempotent: tools whose binary and schema sidecar are already present
    /// are skipped.
    Sync,
}

#[derive(Subcommand)]
enum ExperimentCmd {
    /// Create a new experiment (interactive wizard).
    New,
    /// Run an experiment against the current checkout.
    Run {
        /// Experiment name (matches .wezel/experiments/<name>/).
        #[arg(add = ArgValueCandidates::new(complete_experiments))]
        experiment: String,
        /// Output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Human)]
        output_format: OutputFormat,
        /// Include per-step measurements in human-readable output.
        #[arg(short = 'v', long)]
        verbose: bool,
        /// Persist the run under `.wezel/runs/<experiment>/<id>/run.json`.
        #[arg(
            long,
            action = clap::ArgAction::Set,
            value_parser = clap::builder::BoolishValueParser::new(),
            default_value = "yes",
            value_name = "yes|no",
        )]
        save: bool,
        /// Fiflok run id to place in `report.json` inside the saved run dir.
        #[arg(long, value_name = "ID")]
        run_id: Option<u64>,
    },
    /// List available experiments.
    List,
    /// Validate experiment definitions without running them.
    Lint,
}

#[derive(Clone, Debug, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
}

fn run_result(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", style::stderr_failure(format!("wezel: {e:#}")));
            ExitCode::FAILURE
        }
    }
}

fn resolve_project_dir(project_dir: Option<PathBuf>) -> PathBuf {
    project_dir.unwrap_or_else(|| std::env::current_dir().expect("getting current directory"))
}

fn make_workspace(project_dir: PathBuf) -> anyhow::Result<wezel_bench::Workspace> {
    let tool_store = wezel_bench::Workspace::default_tool_store()?;
    wezel_bench::Workspace::discover(project_dir, tool_store)
}

/// Machine-readable result of `wezel experiment run --output-format json`.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct RunCommandOutput<'a> {
    #[serde(flatten)]
    output: &'a wezel_bench::run::ExperimentRunOutput,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_dir: Option<String>,
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp(None)
        .init();

    clap_complete::CompleteEnv::with_factory(Cli::command).complete();

    let cli = Cli::parse();
    let project_dir = resolve_project_dir(cli.project_dir);

    match cli.command {
        Command::Project { cmd } => match cmd {
            ProjectCmd::Init { server_url } => {
                run_result(init_cmd(&project_dir, server_url.as_deref()))
            }
            ProjectCmd::Tool { cmd } => match cmd {
                ToolCmd::Sync => run_result((|| -> anyhow::Result<()> {
                    let ws = make_workspace(project_dir)?;
                    tool_sync(&ws)
                })()),
            },
            ProjectCmd::Status => run_result(cmd::status_cmd(&project_dir)),
        },

        Command::Completions => {
            let shell = std::env::var("SHELL").unwrap_or_default();
            let exe = std::env::current_exe()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| "wezel".to_string());
            if shell.contains("fish") {
                println!("COMPLETE=fish \"{exe}\" | source");
            } else if shell.contains("bash") {
                println!("eval \"$(COMPLETE=bash \"{exe}\")\"");
            } else {
                println!("eval \"$(COMPLETE=zsh \"{exe}\")\"");
            }
            ExitCode::SUCCESS
        }

        Command::Experiment { cmd } => match cmd {
            ExperimentCmd::New => {
                let name: String = dialoguer::Input::new()
                    .with_prompt("Experiment name")
                    .interact_text()
                    .unwrap();
                let description: String = dialoguer::Input::new()
                    .with_prompt("Description (optional)")
                    .allow_empty(true)
                    .interact_text()
                    .unwrap();
                let description = if description.is_empty() {
                    None
                } else {
                    Some(description)
                };
                run_result(wezel_bench::new::create_experiment(
                    &name,
                    description.as_deref(),
                    &project_dir,
                ))
            }
            ExperimentCmd::Run {
                experiment,
                output_format,
                verbose,
                save,
                run_id,
            } => run_result((|| -> anyhow::Result<()> {
                let ws = make_workspace(project_dir)?;
                let mut fetcher = fetcher::ConfigFetcher::new(&ws)?;
                let mut caching = wezel_bench::fetch::CachingFetcher::new(&mut fetcher);
                let reporter =
                    (output_format == OutputFormat::Human).then(progress::IndicatifReporter::new);

                let branch = wezel_bench::git::current_branch(&ws.project_dir)
                    .ok()
                    .flatten();
                let dirty = wezel_bench::git::is_dirty(&ws.project_dir).unwrap_or(false);
                // Read the baseline before this run is saved, or it would find
                // itself.
                let previous = wezel_bench::run::load_previous_run(&ws, &experiment);
                let started_at = wezel_bench::run::utc_timestamp_rfc3339();
                let t0 = std::time::Instant::now();

                let completed = wezel_bench::run::run_experiment(
                    &experiment,
                    &ws,
                    Some(&mut caching),
                    reporter
                        .as_ref()
                        .map(|r| r as &dyn wezel_bench::run::RunReporter),
                )?;
                let wezel_bench::run::CompletedRun {
                    steps,
                    summaries: summary_defs,
                    plan,
                    attachment_files,
                    measuring_by_step,
                    attachment_dir: _attachment_dir,
                } = completed;
                let duration_ms = u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX);
                let commit = wezel_bench::git::current_sha(&ws.project_dir)?;
                let summaries = wezel_bench::run::compute_summaries(&steps, &summary_defs);
                let runner_report = run_id.map(|run_id| wezel_types::ExperimentRunReport {
                    run_id,
                    steps: steps.clone(),
                    summaries: summary_defs.clone(),
                });

                let saved = wezel_bench::run::SavedRun {
                    schema_version: wezel_bench::run::SAVED_RUN_SCHEMA_VERSION,
                    wezel_version: env!("CARGO_PKG_VERSION").to_string(),
                    started_at,
                    duration_ms,
                    dirty,
                    branch,
                    output: wezel_bench::run::ExperimentRunOutput {
                        experiment,
                        commit,
                        steps,
                        summaries,
                    },
                };

                let saved_dir = save
                    .then(|| {
                        wezel_bench::run::save_run_with_attachments(&ws, &saved, &attachment_files)
                    })
                    .transpose()?;

                if let (Some(dir), Some(report)) = (saved_dir.as_ref(), runner_report.as_ref()) {
                    report_artifacts::write_report_json(dir, report)?;
                }

                let run_dir = saved_dir.as_ref().map(|dir| dir.display().to_string());

                match output_format {
                    OutputFormat::Json => {
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&RunCommandOutput {
                                output: &saved.output,
                                status: "complete",
                                run_id,
                                run_dir,
                            })
                            .unwrap()
                        );
                    }
                    OutputFormat::Human => {
                        report::print_run(
                            &saved,
                            &plan,
                            &summary_defs,
                            &measuring_by_step,
                            previous.as_ref(),
                            verbose,
                        );
                        // Trailer, not a banner: the path is only useful after
                        // you've read the numbers.
                        if let Some(dir) = saved_dir {
                            report::print_saved_at(&dir, &ws.project_dir);
                        }
                    }
                }
                Ok(())
            })()),
            ExperimentCmd::List => run_result(wezel_bench::run::list_experiments(&project_dir)),
            ExperimentCmd::Lint => run_result((|| -> anyhow::Result<()> {
                let ws = make_workspace(project_dir)?;
                let mut fetcher = fetcher::ConfigFetcher::read_only(&ws)?;
                let mut caching = wezel_bench::fetch::CachingFetcher::new(&mut fetcher);
                wezel_bench::lint::run_lint(&ws, Some(&mut caching))
            })()),
        },
    }
}

fn tool_sync(ws: &wezel_bench::Workspace) -> anyhow::Result<()> {
    let foragers: Vec<String> = ws.config.tools.foragers.keys().cloned().collect();
    if foragers.is_empty() {
        println!(
            "{}",
            style::warning("No tools declared under [tools.foragers] in .wezel/config.toml.")
        );
        return Ok(());
    }

    let host = wezel_bench::fetch::current_target()
        .ok_or_else(|| anyhow::anyhow!("current platform is not a recognised target triple"))?;
    let targets = &ws.config.tools.targets;
    if targets.is_empty() {
        anyhow::bail!(
            "no targets declared. Add `targets = [\"{host}\"]` under [tools] in \
             .wezel/config.toml (new projects: `wezel project init` does this automatically)"
        );
    }
    if !targets.iter().any(|t| t == host) {
        anyhow::bail!(
            "host target `{host}` is not in [tools] targets. Add it so the lockfile \
             can be populated from this machine."
        );
    }

    let mut fetcher = fetcher::ConfigFetcher::new(ws)?;
    let mut installed = 0usize;
    let mut skipped = 0usize;
    for name in &foragers {
        if sidecar_is_current(ws, name) {
            println!(
                "  {}  {}",
                style::strong(wezel_types::executor_binary_name(name)),
                style::success("up to date")
            );
            skipped += 1;
        } else {
            wezel_bench::fetch::PluginFetcher::fetch(&mut fetcher, name)?;
            installed += 1;
        }
        // Cross-lock every other declared target via the .sha256 sidecar so
        // wezel.lock is identical on every machine (host was locked by the
        // install above).
        for target in targets {
            if target == host {
                continue;
            }
            fetcher.lock_target(name, target)?;
        }
    }

    write_schema_bundle(ws, &foragers)?;

    println!(
        "\n{}",
        style::success(format!("{installed} installed, {skipped} up to date."))
    );
    Ok(())
}

/// Read every installed forager's sidecar, build the editor-facing bundle,
/// and write it to `.wezel/schema.json`. Called after `tool_sync` installs
/// or refreshes plugins so the bundle is always in sync with what's on disk.
fn write_schema_bundle(ws: &wezel_bench::Workspace, foragers: &[String]) -> anyhow::Result<()> {
    let mut sidecars = Vec::with_capacity(foragers.len());
    for name in foragers {
        let binary = ws.resolve_plugin(name).with_context(|| {
            format!("{} not installed", wezel_types::executor_binary_name(name))
        })?;
        let path = wezel_bench::Workspace::schema_sidecar_path(&binary);
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("reading sidecar {}", path.display()))?;
        let schema: wezel_types::ForagerSchema = serde_json::from_str(&raw)
            .with_context(|| format!("parsing sidecar {}", path.display()))?;
        sidecars.push(schema);
    }

    let bundle = wezel_bench::build_bundle(sidecars);
    let bundle_path = ws.bundle_schema_path();
    if let Some(parent) = bundle_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let body = serde_json::to_string_pretty(&bundle).context("serialising schema bundle")?;
    std::fs::write(&bundle_path, body)
        .with_context(|| format!("writing {}", bundle_path.display()))?;
    println!(
        "  {} {}",
        style::success("wrote"),
        style::muted(bundle_path.display())
    );
    Ok(())
}

/// True only when the cached sidecar exists and matches the current
/// [`wezel_types::ForagerSchema`] shape. A stale-format file is treated as
/// missing so `tool sync` re-fetches it.
fn sidecar_is_current(ws: &wezel_bench::Workspace, forager: &str) -> bool {
    let Some(binary) = ws.resolve_plugin(forager) else {
        return false;
    };
    let path = wezel_bench::Workspace::schema_sidecar_path(&binary);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return false;
    };
    serde_json::from_str::<wezel_types::ForagerSchema>(&raw).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn experiment_next_is_not_a_command() {
        assert!(Cli::try_parse_from(["wezel", "experiment", "next"]).is_err());
    }

    #[test]
    fn observe_is_not_a_command() {
        assert!(Cli::try_parse_from(["wezel", "observe", "exec", "cargo"]).is_err());
    }

    #[test]
    fn experiment_run_accepts_runner_report_outputs() {
        let cli = Cli::try_parse_from([
            "wezel",
            "experiment",
            "run",
            "build",
            "--run-id",
            "7",
            "--output-format",
            "json",
        ])
        .unwrap();

        let Command::Experiment {
            cmd:
                ExperimentCmd::Run {
                    experiment,
                    run_id,
                    output_format,
                    ..
                },
        } = cli.command
        else {
            panic!("expected experiment run command");
        };
        assert_eq!(experiment, "build");
        assert_eq!(run_id, Some(7));
        assert_eq!(output_format, OutputFormat::Json);
    }

    #[test]
    fn run_id_can_be_used_without_saving() {
        let cli = Cli::try_parse_from([
            "wezel",
            "experiment",
            "run",
            "build",
            "--run-id",
            "7",
            "--save",
            "no",
        ])
        .unwrap();

        let Command::Experiment {
            cmd: ExperimentCmd::Run { run_id, save, .. },
        } = cli.command
        else {
            panic!("expected experiment run command");
        };
        assert_eq!(run_id, Some(7));
        assert!(!save);
    }

    #[test]
    fn run_dispatched_is_not_a_command() {
        assert!(Cli::try_parse_from(["wezel", "experiment", "run-dispatched"]).is_err());
    }
}
