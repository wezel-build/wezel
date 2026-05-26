mod cmd;
mod config;
mod daemon;
mod fetcher;
mod flush;
mod pheromone_mgr;
mod progress;
mod queue;
mod shell;

use anyhow::Context as _;
use log::{debug, warn};

use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::engine::{ArgValueCandidates, CompletionCandidate};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use cmd::{alias_cmd, health_cmd, init_cmd};
use flush::flush_events;
use wezel_types::{BuildEvent, PheromoneOutput};

pub(crate) fn wezel_dir() -> PathBuf {
    dirs::home_dir()
        .expect("could not determine home directory")
        .join(".wezel")
}

pub(crate) fn pheromones_dir() -> PathBuf {
    let exe = std::env::current_exe().expect("could not determine wezel executable path");
    let bin_dir = exe
        .parent()
        .expect("wezel executable has no parent directory");
    bin_dir.join("pheromones")
}

fn handler_path(handler: &str) -> PathBuf {
    pheromones_dir().join(format!("pheromone-{handler}"))
}

fn pheromone_out_path(tool: &str, id: &uuid::Uuid) -> PathBuf {
    std::env::temp_dir().join(format!("pheromone-{tool}-{id}.json"))
}

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

fn detect_commit() -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if sha.is_empty() { None } else { Some(sha) }
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

fn detect_platform() -> String {
    let os = std::env::consts::OS;
    let arch = std::env::consts::ARCH;

    let os_version = detect_os_version().unwrap_or_default();
    let chip = detect_chip().unwrap_or_else(|| arch.to_string());

    let os_name = match os {
        "macos" => "macOS",
        "linux" => "Linux",
        "windows" => "Windows",
        other => other,
    };

    if os_version.is_empty() {
        format!("{os_name}, {chip}")
    } else {
        format!("{os_name} {os_version}, {chip}")
    }
}

fn detect_os_version() -> Option<String> {
    match std::env::consts::OS {
        "macos" => {
            let out = std::process::Command::new("sw_vers")
                .arg("-productVersion")
                .stderr(std::process::Stdio::null())
                .output()
                .ok()?;
            let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if v.is_empty() { None } else { Some(v) }
        }
        "linux" => {
            // Try /etc/os-release
            let content = std::fs::read_to_string("/etc/os-release").ok()?;
            for line in content.lines() {
                if let Some(rest) = line.strip_prefix("PRETTY_NAME=") {
                    return Some(rest.trim_matches('"').to_string());
                }
            }
            None
        }
        _ => None,
    }
}

fn detect_chip() -> Option<String> {
    match std::env::consts::OS {
        "macos" => {
            let out = std::process::Command::new("sysctl")
                .arg("-n")
                .arg("machdep.cpu.brand_string")
                .stderr(std::process::Stdio::null())
                .output()
                .ok()?;
            let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        }
        "linux" => {
            let content = std::fs::read_to_string("/proc/cpuinfo").ok()?;
            for line in content.lines() {
                if line.starts_with("model name")
                    && let Some(val) = line.split(':').nth(1)
                {
                    return Some(val.trim().to_string());
                }
            }
            None
        }
        _ => None,
    }
}

fn read_pheromone_output(path: &Path) -> Option<PheromoneOutput> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Block until the pheromone child signals readiness by closing the write end
/// of the pipe (read_fd), then read PHEROMONE_OUT and flush the event.
fn flush_in_background(
    read_fd: i32,
    pheromone_out: &Path,
    config: &config::Config,
    exit_code: i32,
    duration_ms: u64,
) {
    // Block until the pheromone background child closes its write end.
    let mut buf = [0u8; 1];
    unsafe { libc::read(read_fd, buf.as_mut_ptr() as *mut libc::c_void, 1) };
    unsafe { libc::close(read_fd) };

    let pheromone = read_pheromone_output(pheromone_out);
    let _ = std::fs::remove_file(pheromone_out);

    let cwd = std::env::current_dir().unwrap_or_default();
    let timestamp = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        format!("{}", now.as_secs())
    };
    let id = uuid::Uuid::new_v4();
    let tool: String = pheromone
        .as_ref()
        .map(|p| p.tool.clone())
        .unwrap_or_else(|| "cargo".to_string());

    let event = BuildEvent {
        project_id: config.project_id,
        upstream: detect_upstream(),
        commit: detect_commit(),
        cwd: cwd.display().to_string(),
        user: config.username.clone(),
        platform: detect_platform(),
        timestamp,
        duration_ms,
        exit_code,
        pheromone,
    };

    debug!("persisting event {tool}-{id}");
    persist_event(&tool, &id, &event);

    debug!("flushing events to {:?}", config.server_url);
    if let Err(e) = flush_events(config) {
        warn!("flush failed: {e}");
    }
}

fn persist_event(tool: &str, id: &uuid::Uuid, event: &BuildEvent) {
    let dir = wezel_dir().join("events");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{tool}-{id}.json"));
    if let Ok(json) = serde_json::to_string(event) {
        let _ = std::fs::write(path, json);
    }
}

fn exec_cmd(args: &[String]) -> anyhow::Result<ExitCode> {
    if args.is_empty() {
        anyhow::bail!("Usage: wezel exec -- <tool> [args...]");
    }

    let tool = &args[0];
    let tool_args = &args[1..];

    let handler = handler_path(tool);
    let (program, program_args): (&std::ffi::OsStr, &[String]) = if handler.is_file() {
        debug!("using pheromone handler: {}", handler.display());
        (handler.as_os_str(), tool_args)
    } else {
        debug!(
            "no pheromone-{tool} found in {}, passing through",
            pheromones_dir().display()
        );
        (std::ffi::OsStr::new(tool.as_str()), tool_args)
    };

    let cwd = std::env::current_dir().unwrap_or_default();

    let project = config::discover(&cwd);
    if project.is_none() {
        debug!("no project config found, pure passthrough for `{tool}`");
        let status = std::process::Command::new(program)
            .args(program_args)
            .status();
        return match status {
            Ok(s) => Ok(ExitCode::from(s.code().unwrap_or(1) as u8)),
            Err(e) => {
                eprintln!("wezel: failed to execute `{tool}`: {e}");
                Ok(ExitCode::from(127))
            }
        };
    }
    let (wezel_dir, config) = project.unwrap();
    debug!("project .wezel dir: {}", wezel_dir.display());

    let id = uuid::Uuid::new_v4();
    let pheromone_out = pheromone_out_path(tool, &id);

    let start = Instant::now();

    // Create a pipe. The write end is passed to the pheromone handler via
    // PHEROMONE_READY_FD; its background child closes it when PHEROMONE_OUT
    // is written. Our background child blocks on the read end.
    let mut pipe_fds = [0i32; 2];
    let pipe_ok = unsafe { libc::pipe(pipe_fds.as_mut_ptr()) } == 0;
    let (read_fd, write_fd) = if pipe_ok {
        (pipe_fds[0], pipe_fds[1])
    } else {
        (-1, -1)
    };

    debug!(
        "spawning pheromone handler: {} {:?}",
        program.to_string_lossy(),
        program_args
    );
    let mut cmd = std::process::Command::new(program);
    cmd.args(program_args).env("PHEROMONE_OUT", &pheromone_out);
    if pipe_ok {
        cmd.env("PHEROMONE_READY_FD", write_fd.to_string());
    }
    let status = cmd.status();

    let duration_ms = start.elapsed().as_millis() as u64;

    let (exit_code, process_exit_code) = match &status {
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            (code, ExitCode::from(code as u8))
        }
        Err(_) => (127, ExitCode::from(127)),
    };

    if let Err(e) = &status {
        eprintln!("wezel: failed to execute `{tool}`: {e}");
    }

    // Close our copy of write_fd so that when the pheromone child closes its
    // copy, the read end sees EOF.
    if pipe_ok {
        unsafe { libc::close(write_fd) };
    }

    // Fork so the parent can return the exit code to the shell immediately.
    // The child blocks on the pipe read end waiting for pheromone's background
    // child to finish, then persists + flushes the event.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        // Fork failed — fall back to synchronous path.
        debug!("fork failed, flushing synchronously");
        flush_in_background(read_fd, &pheromone_out, &config, exit_code, duration_ms);
    } else if pid == 0 {
        // Child: wait for signal then flush.
        debug!("child: waiting on pipe for pheromone to finish, then flushing events");
        flush_in_background(read_fd, &pheromone_out, &config, exit_code, duration_ms);
        unsafe { libc::_exit(0) };
    } else {
        // Parent: close the read end and return immediately.
        if pipe_ok {
            unsafe { libc::close(read_fd) };
        }
    }

    Ok(process_exit_code)
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
    /// Enable shell completions for wezel commands.
    Completions,
    /// Passive build observation: aliases, event flushing, health.
    #[command(hide = true)]
    Observe {
        #[command(subcommand)]
        cmd: ObserveCmd,
    },
}

#[derive(Subcommand)]
enum ProjectCmd {
    /// Initialize wezel in the current project.
    ///
    /// Creates `.wezel/config.toml` in the current directory.
    /// Options not passed on the command line are prompted interactively.
    Init {
        /// Burrow API URL to push build timings to.
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
    },
    /// List available experiments.
    List,
    /// Validate experiment definitions without running them.
    Lint,
    /// Manage the experiment daemon (polls server for jobs).
    Daemon {
        #[command(subcommand)]
        cmd: ExperimentDaemonCmd,
    },
}

#[derive(Subcommand)]
enum ExperimentDaemonCmd {
    /// Start polling the server for queued experiment jobs and run them.
    Start {
        /// Path to the repository to check out and run experiments in.
        #[arg(long)]
        repo_dir: PathBuf,
        /// Seconds to wait between polls when no job is available.
        #[arg(long, default_value = "10")]
        poll_interval: u64,
    },
    /// Run a single standalone pass (no Burrow server).
    ///
    /// Executes all experiments against the target branch, manages state on
    /// a data branch, performs bisection if needed, and outputs a JSON report.
    Standalone {
        /// Path to the repository.
        #[arg(long)]
        repo_dir: PathBuf,
        /// Branch to track for regressions.
        #[arg(long, default_value = "main")]
        branch: String,
        /// Regression threshold as a percentage.
        #[arg(long, default_value = "10")]
        threshold: f64,
    },
    /// Show current experiment daemon status and active job.
    Status,
}

#[derive(Clone, Debug, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Human,
    Json,
}

#[derive(Subcommand)]
enum ObserveCmd {
    /// Manage tool aliases.
    ///
    /// Without arguments, ensures the shell hook is installed and shows status.
    /// `wezel observe alias cargo`              — alias cargo → pheromone-cargo
    /// `wezel observe alias cargo-nightly cargo` — alias cargo-nightly → pheromone-cargo
    /// `wezel observe alias cargo --remove`     — remove the cargo alias
    Alias {
        /// Shell alias name (e.g. cargo, cargo-nightly).
        name: Option<String>,
        /// Pheromone handler to route to (defaults to the alias name).
        handler: Option<String>,
        /// Remove the alias instead of installing it.
        #[arg(long)]
        remove: bool,
    },
    /// Check wezel health: pheromones, config, server connectivity.
    Health,
    /// Run a tool, recording pre/post build events.
    Exec {
        /// The tool and its arguments (use `--` before them).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run the observation daemon (flush queue + update pheromones).
    ///
    /// Normally spawned automatically. Use `--foreground` to run in the
    /// foreground (used internally by the auto-spawn path).
    Daemon {
        /// Run in the foreground (do not double-fork).
        #[arg(long)]
        foreground: bool,
    },
    /// One-shot flush: send queued events to server and check pheromone updates.
    Sync,
}

fn run_result(result: anyhow::Result<()>) -> ExitCode {
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("wezel: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn resolve_project_dir(project_dir: Option<PathBuf>) -> PathBuf {
    project_dir.unwrap_or_else(|| std::env::current_dir().expect("getting current directory"))
}

fn make_workspace(project_dir: PathBuf) -> anyhow::Result<wezel_bench::Workspace> {
    let plugin_dir = wezel_bench::Workspace::default_plugin_dir()?;
    wezel_bench::Workspace::discover(project_dir, plugin_dir)
}

fn print_human_report(
    saved: &wezel_bench::run::SavedRun,
    summary_defs: &[wezel_types::SummaryDef],
    verbose: bool,
) {
    let output = &saved.output;
    let short = &output.commit[..7.min(output.commit.len())];
    let branch = saved.branch.as_deref().unwrap_or("(detached)");
    let dirty = if saved.dirty { " *" } else { "" };
    let dur = format_duration(saved.duration_ms);

    println!(
        "{}  ·  {short} on {branch}{dirty}  ·  {dur}",
        output.experiment
    );

    if output.summaries.is_empty() {
        println!();
        println!("(no summaries)");
    } else {
        let mut entries: Vec<_> = output.summaries.iter().collect();
        entries.sort_by(|a, b| a.0.cmp(b.0));

        let name_w = entries
            .iter()
            .map(|(n, _)| n.len())
            .max()
            .unwrap_or(0)
            .max("SUMMARY".len());

        println!();
        println!("{:<name_w$}  VALUE", "SUMMARY");
        for (name, sv) in entries {
            let samples = summary_defs
                .iter()
                .find(|d| &d.name == name)
                .map(|d| d.matching_values(&output.steps))
                .unwrap_or_default();
            if samples.len() > 1 {
                let min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
                let max = samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                println!(
                    "{name:<name_w$}  {}  (n={}, min={}, max={})",
                    sv.value,
                    samples.len(),
                    min,
                    max,
                );
            } else {
                println!("{name:<name_w$}  {}", sv.value);
            }
        }
    }

    if verbose {
        println!();
        println!("Outcomes:");
        for report in &output.steps {
            if report.measurements.is_empty() {
                println!("  {} — (none)", report.step);
                continue;
            }
            for m in &report.measurements {
                let mut line = format!("  {}.{} = {}", report.step, m.name, m.value);
                if !m.tags.is_empty() {
                    let mut tags: Vec<_> = m.tags.iter().collect();
                    tags.sort_by(|a, b| a.0.cmp(b.0));
                    let joined: Vec<_> = tags.iter().map(|(k, v)| format!("{k}={v}")).collect();
                    line.push_str(&format!(" [{}]", joined.join(", ")));
                }
                println!("{line}");
            }
        }
    }
}

fn format_duration(ms: u64) -> String {
    let s = ms as f64 / 1000.0;
    if s < 60.0 {
        format!("{s:.1}s")
    } else if s < 3600.0 {
        let m = (s / 60.0) as u64;
        let rs = s - (m as f64) * 60.0;
        format!("{m}m{rs:.0}s")
    } else {
        let h = (s / 3600.0) as u64;
        let m = ((s - (h as f64) * 3600.0) / 60.0) as u64;
        format!("{h}h{m}m")
    }
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
            let Some(shell) = shell::Shell::detect() else {
                eprintln!("wezel: could not detect shell from $SHELL");
                return ExitCode::FAILURE;
            };
            if let Err(e) = shell::ensure_shell_hook(shell) {
                eprintln!("wezel: {e}");
                return ExitCode::FAILURE;
            }
            let aliases = cmd::load_aliases().unwrap_or_default().aliases;
            if let Err(e) = shell::sync_init_script(shell, &aliases) {
                eprintln!("wezel: {e}");
                return ExitCode::FAILURE;
            }
            println!("Shell completions enabled. Restart your shell or run:");
            let shell_name = match shell {
                shell::Shell::Zsh => "zsh",
                shell::Shell::Bash => "bash",
                shell::Shell::Fish => "fish",
            };
            println!("  source ~/.wezel/init.{shell_name}");
            ExitCode::SUCCESS
        }

        Command::Experiment { cmd } => {
            match cmd {
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
                } => run_result((|| -> anyhow::Result<()> {
                    let ws = make_workspace(project_dir)?;
                    let mut fetcher = fetcher::ConfigFetcher::new(&ws)?;
                    let mut caching = wezel_bench::fetch::CachingFetcher::new(&mut fetcher);
                    let reporter = (output_format == OutputFormat::Human)
                        .then(progress::IndicatifReporter::new);

                    let branch = wezel_bench::git::current_branch(&ws.project_dir)
                        .ok()
                        .flatten();
                    let dirty = wezel_bench::git::is_dirty(&ws.project_dir).unwrap_or(false);
                    let started_at = wezel_bench::run::utc_timestamp_rfc3339();
                    let t0 = std::time::Instant::now();

                    let (steps, summary_defs) = wezel_bench::run::run_experiment(
                        &experiment,
                        &ws,
                        Some(&mut caching),
                        reporter
                            .as_ref()
                            .map(|r| r as &dyn wezel_bench::run::RunReporter),
                    )?;
                    let duration_ms = u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX);
                    let commit = wezel_bench::git::current_sha(&ws.project_dir)?;
                    let summaries = wezel_bench::run::compute_summaries(&steps, &summary_defs);

                    let saved = wezel_bench::run::SavedRun {
                        schema_version: 1,
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

                    if save {
                        let dir = wezel_bench::run::save_run(&ws, &saved)?;
                        eprintln!("Saved: {}", dir.display());
                    }

                    match output_format {
                        OutputFormat::Json => {
                            println!("{}", serde_json::to_string_pretty(&saved.output).unwrap());
                        }
                        OutputFormat::Human => {
                            print_human_report(&saved, &summary_defs, verbose);
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
                ExperimentCmd::Daemon { cmd: daemon_cmd } => match daemon_cmd {
                    ExperimentDaemonCmd::Start {
                        repo_dir,
                        poll_interval,
                    } => run_result((|| -> anyhow::Result<()> {
                        let ws = make_workspace(repo_dir)?;
                        let mut fetcher = fetcher::ConfigFetcher::new(&ws)?;
                        wezel_bench::daemon::run_start(&ws, poll_interval, Some(&mut fetcher))
                    })()),
                    ExperimentDaemonCmd::Standalone {
                        repo_dir,
                        branch,
                        threshold,
                    } => {
                        run_result((|| -> anyhow::Result<()> {
                            let ws = make_workspace(repo_dir)?;
                            let data_branch = ws.config.target.data_branch().map(ToOwned::to_owned).context(
                        "Standalone mode is not available when Burrow server is configured",
                    )?;
                            let mut fetcher = fetcher::ConfigFetcher::new(&ws)?;
                            let mut caching = wezel_bench::fetch::CachingFetcher::new(&mut fetcher);
                            let report = wezel_bench::standalone::run_standalone(
                                &ws,
                                &data_branch,
                                &branch,
                                threshold,
                                Some(&mut caching),
                            )?;
                            println!("{}", serde_json::to_string_pretty(&report).unwrap());
                            Ok(())
                        })())
                    }
                    ExperimentDaemonCmd::Status => run_result(wezel_bench::daemon::run_status()),
                },
            }
        }

        Command::Observe { cmd } => match cmd {
            ObserveCmd::Alias {
                name,
                handler,
                remove,
            } => run_result(alias_cmd(name.as_deref(), handler.as_deref(), remove)),
            ObserveCmd::Health => run_result(health_cmd()),
            ObserveCmd::Exec { args } => match exec_cmd(&args) {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("wezel: {e}");
                    ExitCode::FAILURE
                }
            },
            ObserveCmd::Daemon { foreground } => {
                if foreground {
                    daemon::run_daemon();
                } else if let Err(e) = daemon::spawn_detached() {
                    eprintln!("wezel: failed to spawn daemon: {e}");
                    return ExitCode::FAILURE;
                }
                ExitCode::SUCCESS
            }
            ObserveCmd::Sync => {
                let cwd = std::env::current_dir().unwrap_or_default();
                let Some((_, config)) = config::discover(&cwd) else {
                    eprintln!("wezel: no project config found (run `wezel project init` first)");
                    return ExitCode::FAILURE;
                };
                let Some(ref server_url) = config.server_url else {
                    eprintln!(
                        "wezel: server_url not configured (set WEZEL_BURROW_URL or add server_url to .wezel/config.toml)"
                    );
                    return ExitCode::FAILURE;
                };
                let n = queue::flush_queue(server_url);
                println!("wezel sync: flushed {n} event(s)");
                pheromone_mgr::update_pheromones(server_url, &pheromones_dir());
                println!("wezel sync: pheromone check done");
                ExitCode::SUCCESS
            }
        },
    }
}

fn tool_sync(ws: &wezel_bench::Workspace) -> anyhow::Result<()> {
    let foragers: Vec<String> = ws.config.tools.foragers.keys().cloned().collect();
    if foragers.is_empty() {
        println!("No tools declared under [tools.foragers] in .wezel/config.toml.");
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
        if ws.resolve_plugin(name).is_some() && sidecar_is_current(ws, name) {
            println!("  forager-{name}  up to date");
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

    println!("\n{installed} installed, {skipped} up to date.");
    Ok(())
}

/// Read every installed forager's sidecar, build the editor-facing bundle,
/// and write it to `.wezel/schema.json`. Called after `tool_sync` installs
/// or refreshes plugins so the bundle is always in sync with what's on disk.
fn write_schema_bundle(ws: &wezel_bench::Workspace, foragers: &[String]) -> anyhow::Result<()> {
    let mut sidecars = Vec::with_capacity(foragers.len());
    for name in foragers {
        let path = ws.schema_path(name);
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
    println!("  wrote {}", bundle_path.display());
    Ok(())
}

/// True only when the cached sidecar exists and matches the current
/// [`wezel_types::ForagerSchema`] shape. A stale-format file is treated as
/// missing so `tool sync` re-fetches it.
fn sidecar_is_current(ws: &wezel_bench::Workspace, forager: &str) -> bool {
    let path = ws.schema_path(forager);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return false;
    };
    serde_json::from_str::<wezel_types::ForagerSchema>(&raw).is_ok()
}
