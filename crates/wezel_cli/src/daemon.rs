//! Background daemon for wezel.
//!
//! The daemon:
//!   1. Every 30s: Flushes `~/.wezel/queue/` to burrow.
//!   2. Every 5min: Checks for pheromone binary updates.
//!   3. Auto-exits after 5min of idle (no queue files seen).
//!   4. Writes its PID to `~/.wezel/wezel.pid`.
//!
//! The daemon detects whether another instance is already running by
//! checking the PID file before starting.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::config;
use crate::pheromone_mgr::update_pheromones;
use crate::queue::{flush_queue, queue_dir};

const FLUSH_INTERVAL: Duration = Duration::from_secs(30);
const UPDATE_INTERVAL: Duration = Duration::from_secs(5 * 60);
const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);

fn pid_file() -> PathBuf {
    crate::wezel_dir().join("wezel.pid")
}

fn read_pid() -> Option<u32> {
    std::fs::read_to_string(pid_file())
        .ok()
        .and_then(|s| s.trim().parse().ok())
}

fn write_pid() {
    let dir = crate::wezel_dir();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(pid_file(), std::process::id().to_string());
}

fn remove_pid() {
    let _ = std::fs::remove_file(pid_file());
}

/// Returns `true` if another daemon appears to be running.
#[cfg(unix)]
pub fn is_running() -> bool {
    let Some(pid) = read_pid() else { return false };
    // Send signal 0 to check if process exists.
    (unsafe { libc::kill(pid as libc::pid_t, 0) } == 0)
}

#[cfg(not(unix))]
pub fn is_running() -> bool {
    // On non-Unix just check if PID file exists (best effort).
    pid_file().exists()
}

/// Run the daemon loop. Does not return until idle timeout or signal.
pub fn run_daemon() {
    if is_running() {
        log::info!("daemon: another instance is already running, exiting");
        return;
    }

    write_pid();
    // Remove PID file on exit.
    let _guard = PidGuard;

    log::info!("daemon: starting (pid {})", std::process::id());

    let pheromone_dir = crate::pheromones_dir();
    let mut last_flush = Instant::now()
        .checked_sub(FLUSH_INTERVAL)
        .unwrap_or(Instant::now());
    let mut last_update = Instant::now()
        .checked_sub(UPDATE_INTERVAL)
        .unwrap_or(Instant::now());
    let mut last_activity = Instant::now();

    loop {
        let now = Instant::now();

        // Find a project config so we have a server_url.
        // Walk up from CWD or use first found config.
        let config = find_any_config();

        // Flush queue if interval has elapsed.
        if now.duration_since(last_flush) >= FLUSH_INTERVAL {
            last_flush = now;
            if let Some(ref cfg) = config
                && let Some(ref server_url) = cfg.server_url
            {
                let n = flush_queue(server_url);
                if n > 0 {
                    log::info!("daemon: flushed {n} event(s) to burrow");
                    last_activity = now;
                }
            }
        }

        // Update pheromones if interval has elapsed.
        if now.duration_since(last_update) >= UPDATE_INTERVAL {
            last_update = now;
            if let Some(ref cfg) = config
                && let Some(ref server_url) = cfg.server_url
            {
                log::debug!("daemon: checking pheromone updates");
                update_pheromones(server_url, &pheromone_dir);
            }
        }

        // Check for idle timeout.
        let queue_empty = queue_dir()
            .read_dir()
            .map(|mut d| d.next().is_none())
            .unwrap_or(true);
        if queue_empty && now.duration_since(last_activity) >= IDLE_TIMEOUT {
            log::info!("daemon: idle timeout, exiting");
            break;
        }

        std::thread::sleep(Duration::from_secs(5));
    }
}

/// Find a Config from any `.wezel/config.toml` walking up from CWD or home.
fn find_any_config() -> Option<config::Config> {
    // Try CWD first.
    if let Ok(cwd) = std::env::current_dir()
        && let Some((_, cfg)) = config::discover(&cwd)
    {
        return Some(cfg);
    }
    // Try home dir.
    if let Some(home) = dirs::home_dir()
        && let Some((_, cfg)) = config::discover(&home)
    {
        return Some(cfg);
    }
    None
}

struct PidGuard;
impl Drop for PidGuard {
    fn drop(&mut self) {
        remove_pid();
    }
}

/// Spawn the daemon as a detached background process.
///
/// Returns `Ok(())` if the daemon was spawned (or was already running).
#[cfg(unix)]
pub fn spawn_detached() -> anyhow::Result<()> {
    if is_running() {
        log::debug!("daemon: already running, not spawning");
        return Ok(());
    }

    let exe = std::env::current_exe()?;

    // Double-fork so the child becomes a session leader, independent of our
    // process group and terminal.
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        anyhow::bail!("fork failed");
    }
    if pid > 0 {
        // Parent — return immediately.
        return Ok(());
    }

    // Child: become session leader.
    unsafe { libc::setsid() };

    let pid2 = unsafe { libc::fork() };
    if pid2 < 0 {
        unsafe { libc::_exit(1) };
    }
    if pid2 > 0 {
        // First child exits.
        unsafe { libc::_exit(0) };
    }

    // Grandchild: run daemon.
    let _ = std::process::Command::new(exe)
        .arg("daemon")
        .arg("--foreground")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();

    unsafe { libc::_exit(0) };
}

#[cfg(not(unix))]
pub fn spawn_detached() -> anyhow::Result<()> {
    let exe = std::env::current_exe()?;
    std::process::Command::new(exe)
        .arg("daemon")
        .arg("--foreground")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}
