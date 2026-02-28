mod cmd;
mod flush;
mod shell;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use cmd::alias_cmd;
use flush::flush_events;
use pheromone_types::{BuildEvent, PheromoneOutput};

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

fn events_dir() -> PathBuf {
    wezel_dir().join("events")
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

fn read_pheromone_output(path: &std::path::Path) -> Option<PheromoneOutput> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn persist_event(tool: &str, id: &uuid::Uuid, event: &BuildEvent) {
    let dir = events_dir();
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
        (handler.as_os_str(), tool_args)
    } else {
        eprintln!(
            "wezel warning: pheromone-{tool} not found in {}, passing through to `{tool}`",
            pheromones_dir().display()
        );
        (std::ffi::OsStr::new(tool.as_str()), tool_args)
    };

    // Set up temp file for pheromone handler communication.
    let id = uuid::Uuid::new_v4();
    let pheromone_out = pheromone_out_path(tool, &id);

    let cwd = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();

    let timestamp = {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        // Poor-man's ISO-8601 without pulling in chrono.
        // Format: seconds since epoch (burrow can parse this).
        // If you want real ISO-8601 you'd add chrono/time crate.
        format!("{secs}")
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

    // Read pheromone output (if the handler wrote one) and clean up.
    let pheromone = read_pheromone_output(&pheromone_out);
    let _ = std::fs::remove_file(&pheromone_out);

    let event = BuildEvent {
        upstream: detect_upstream(),
        cwd,
        user: whoami::username(),
        timestamp,
        duration_ms,
        exit_code,
        pheromone,
    };

    persist_event(tool, &id, &event);

    let _ = flush_events();

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
    /// Run a tool, recording pre/post build events.
    Exec {
        /// The tool and its arguments (use `--` before them).
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() -> ExitCode {
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
        Command::Exec { args } => match exec_cmd(&args) {
            Ok(code) => code,
            Err(e) => {
                eprintln!("wezel: {e}");
                ExitCode::FAILURE
            }
        },
    }
}
