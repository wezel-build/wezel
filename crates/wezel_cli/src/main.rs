mod cmd;
mod config;
mod flush;
mod shell;

use log::{debug, warn};

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use cmd::{alias_cmd, health_cmd, setup_cmd};
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
                if line.starts_with("model name") {
                    if let Some(val) = line.split(':').nth(1) {
                        return Some(val.trim().to_string());
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn read_pheromone_output(path: &std::path::Path) -> Option<PheromoneOutput> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
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

    let timestamp = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        format!("{}", now.as_secs())
    };

    let start = Instant::now();

    let status = std::process::Command::new(program)
        .args(program_args)
        .env("PHEROMONE_OUT", &pheromone_out)
        .status();

    let duration_ms = start.elapsed().as_millis() as u64;

    let (exit_code, process_exit_code) = match &status {
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            (code, ExitCode::from(code as u8))
        }
        Err(_) => (127, ExitCode::from(127)),
    };

    let pheromone = read_pheromone_output(&pheromone_out);
    let _ = std::fs::remove_file(&pheromone_out);

    let event = BuildEvent {
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
    persist_event(tool, &id, &event);

    debug!("flushing events to {}", config.burrow_url);
    if let Err(e) = flush_events(&config) {
        warn!("flush failed: {e}");
    }

    if let Err(e) = &status {
        eprintln!("wezel: failed to execute `{tool}`: {e}");
    }

    Ok(process_exit_code)
}

#[derive(Parser)]
#[command(name = "wezel", about = "Lightweight build observer")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Manage tool aliases.
    ///
    /// Without arguments, ensures the shell hook is installed and shows status.
    /// `wezel alias cargo`              — alias cargo → pheromone-cargo
    /// `wezel alias cargo-nightly cargo` — alias cargo-nightly → pheromone-cargo
    /// `wezel alias cargo --remove`     — remove the cargo alias
    Alias {
        /// Shell alias name (e.g. cargo, cargo-nightly).
        name: Option<String>,
        /// Pheromone handler to route to (defaults to the alias name).
        handler: Option<String>,
        /// Remove the alias instead of installing it.
        #[arg(long)]
        remove: bool,
    },
    /// Initialize wezel in the current project.
    ///
    /// Creates `.wezel/config.toml` in the current directory.
    /// Options not passed on the command line are prompted interactively.
    Setup {
        /// Burrow API URL to push build timings to.
        #[arg(long)]
        burrow_url: Option<String>,
    },
    /// Check wezel health: pheromones, config, burrow connectivity.
    Health,
    /// Run a tool, recording pre/post build events.
    Exec {
        /// The tool and its arguments (use `--` before them).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn"))
        .format_timestamp(None)
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Alias {
            name,
            handler,
            remove,
        } => match alias_cmd(name.as_deref(), handler.as_deref(), remove) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("wezel: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Health => match health_cmd() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("wezel: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Setup { burrow_url } => match setup_cmd(burrow_url.as_deref()) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("wezel: {e}");
                ExitCode::FAILURE
            }
        },
        Command::Exec { args } => match exec_cmd(&args) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("wezel: {e}");
                ExitCode::FAILURE
            }
        },
    }
}
