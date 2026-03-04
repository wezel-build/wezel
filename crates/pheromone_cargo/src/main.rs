use std::collections::HashSet;
use std::io::{IsTerminal, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Instant;

use guppy::platform::{EnabledTernary, PlatformSpec};
use wezel_types::{CrateTopo, PheromoneOutput, Profile};

/// Cargo subcommands that trigger a build.
const BUILD_COMMANDS: &[&str] = &[
    "build", "b", "check", "c", "test", "t", "bench", "run", "r", "clippy", "doc", "d", "rustc",
    "rustdoc", "install",
];

/// Commands that default to the release profile.
const RELEASE_DEFAULT_COMMANDS: &[&str] = &["install"];

/// Normalize short aliases to their canonical form.
fn normalize_command(cmd: &str) -> &str {
    match cmd {
        "b" => "build",
        "c" => "check",
        "t" => "test",
        "r" => "run",
        "d" => "doc",
        other => other,
    }
}

fn is_build_command(cmd: &str) -> bool {
    BUILD_COMMANDS.contains(&cmd)
}

struct ParsedArgs {
    command: String,
    profile: Profile,
    packages: Vec<String>,
}

/// Best-effort parse of cargo CLI args to extract command, profile, and packages.
fn parse_args(args: &[String]) -> Option<ParsedArgs> {
    let mut iter = args.iter();
    let mut command: Option<String> = None;
    let mut profile: Option<Profile> = None;
    let mut explicit_debug = false;
    let mut packages: Vec<String> = Vec::new();

    // Skip leading global flags to find the subcommand.
    while let Some(arg) = iter.next() {
        let s = arg.as_str();
        if s.starts_with('-') {
            if matches!(s, "--manifest-path" | "--config" | "-Z" | "--color" | "-C") {
                let _ = iter.next();
            }
            continue;
        }
        command = Some(s.to_string());
        break;
    }

    let command = command?;
    if !is_build_command(&command) {
        return None;
    }

    while let Some(arg) = iter.next() {
        let s = arg.as_str();
        match s {
            "--release" => profile = Some(Profile::Release),
            "--debug" => explicit_debug = true,
            "--profile" => {
                if let Some(val) = iter.next() {
                    profile = Some(match val.as_str() {
                        "release" | "bench" => Profile::Release,
                        _ => Profile::Dev,
                    });
                }
            }
            "-p" | "--package" => {
                if let Some(val) = iter.next() {
                    packages.push(val.clone());
                }
            }
            _ if s.starts_with("--package=") => {
                if let Some(val) = s.strip_prefix("--package=") {
                    packages.push(val.to_string());
                }
            }
            _ if s.starts_with("--profile=") => {
                if let Some(val) = s.strip_prefix("--profile=") {
                    profile = Some(match val {
                        "release" | "bench" => Profile::Release,
                        _ => Profile::Dev,
                    });
                }
            }
            _ => {}
        }
    }

    let canonical = normalize_command(&command);
    let resolved_profile = if explicit_debug {
        Profile::Dev
    } else if let Some(p) = profile {
        p
    } else if RELEASE_DEFAULT_COMMANDS.contains(&canonical) {
        Profile::Release
    } else {
        Profile::Dev
    };

    Some(ParsedArgs {
        command: canonical.to_string(),
        profile: resolved_profile,
        packages,
    })
}

/// Strip ANSI escape sequences from a string so we can pattern-match on the
/// plain text content of cargo's stderr lines.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Consume until the terminating letter of the escape sequence.
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Extract a crate name from a cargo stderr line like:
///   `   Compiling foo v0.1.0 (/path/to/foo)`
/// Handles lines with or without ANSI color codes.
fn parse_compiling_line(line: &str) -> Option<String> {
    let clean = strip_ansi(line);
    let trimmed = clean.trim();
    let rest = trimmed.strip_prefix("Compiling ")?;
    let name = rest.split_whitespace().next()?;
    Some(name.to_string())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageFormat {
    Human,
    Json,
}

fn detect_message_format(args: &[String]) -> MessageFormat {
    let mut iter = args.iter();
    let mut format = MessageFormat::Human;

    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--message-format" => {
                if let Some(val) = iter.next() {
                    format = if val
                        .split(',')
                        .any(|part| part.trim().eq_ignore_ascii_case("json"))
                    {
                        MessageFormat::Json
                    } else {
                        MessageFormat::Human
                    };
                }
            }
            _ if arg.starts_with("--message-format=") => {
                if let Some(val) = arg.strip_prefix("--message-format=") {
                    format = if val
                        .split(',')
                        .any(|part| part.trim().eq_ignore_ascii_case("json"))
                    {
                        MessageFormat::Json
                    } else {
                        MessageFormat::Human
                    };
                }
            }
            _ => {}
        }
    }

    format
}

fn parse_json_message_dirty_crate(line: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let reason = v.get("reason")?.as_str()?;
    match reason {
        "compiler-artifact" | "build-script-executed" => {}
        _ => return None,
    }

    let package_id = v.get("package_id")?.as_str()?;
    let pkg_and_version = package_id.split_once('#')?.1;
    let name = pkg_and_version
        .split_once('@')
        .map(|(pkg, _)| pkg)
        .unwrap_or(pkg_and_version);

    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Find the real `cargo` binary, skipping ourselves.
fn find_real_cargo() -> anyhow::Result<PathBuf> {
    let my_exe = std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::canonicalize(p).ok());

    // $CARGO is set by rustup/cargo and points to the real binary.
    if let Ok(cargo_env) = std::env::var("CARGO") {
        let candidate = PathBuf::from(&cargo_env);
        if candidate.is_file() {
            let canon = std::fs::canonicalize(&candidate).ok();
            if canon != my_exe {
                return Ok(candidate);
            }
        }
    }

    let candidates = which::which_all("cargo")?;
    for candidate in candidates {
        let canon = std::fs::canonicalize(&candidate).ok();
        if canon != my_exe {
            return Ok(candidate);
        }
    }

    anyhow::bail!("could not find real cargo binary")
}

/// Use guppy to extract the workspace dependency graph.
/// Returns an empty vec on failure — we never want this to block the build.
fn extract_graph(cargo: &Path) -> Vec<CrateTopo> {
    let t = Instant::now();
    let graph = match guppy::MetadataCommand::new()
        .cargo_path(cargo)
        .build_graph()
    {
        Ok(g) => {
            log::debug!("cargo metadata took {:?}", t.elapsed());
            g
        }
        Err(e) => {
            log::debug!("cargo metadata failed after {:?}: {e}", t.elapsed());
            return Vec::new();
        }
    };

    let platform = match PlatformSpec::build_target() {
        Ok(p) => p,
        Err(_) => return Vec::new(),
    };

    let workspace_names: std::collections::HashSet<&str> =
        graph.workspace().iter().map(|pkg| pkg.name()).collect();

    let is_active_on_current_platform = |link: &guppy::graph::PackageLink<'_>| {
        matches!(
            link.normal().status().enabled_on(&platform),
            EnabledTernary::Enabled
        ) || matches!(
            link.build().status().enabled_on(&platform),
            EnabledTernary::Enabled
        ) || matches!(
            link.dev().status().enabled_on(&platform),
            EnabledTernary::Enabled
        )
    };

    // Walk transitively from workspace members, following only links active on
    // the current build platform.
    let mut visited = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<guppy::graph::PackageMetadata> =
        graph.workspace().iter().collect();

    while let Some(pkg) = queue.pop_front() {
        if !visited.insert(pkg.name().to_string()) {
            continue;
        }
        for link in pkg.direct_links().filter(&is_active_on_current_platform) {
            let dep_name = link.to().name().to_string();
            if !visited.contains(&dep_name) {
                queue.push_back(link.to());
            }
        }
    }

    // Build CrateTopo for every visited package.
    let mut result = Vec::with_capacity(visited.len());
    for pkg_name in &visited {
        let pkg = match graph.packages().find(|p| p.name() == pkg_name.as_str()) {
            Some(p) => p,
            None => continue,
        };
        // Only include deps that are in our visited set and active on the current platform.
        let deps: Vec<String> = pkg
            .direct_links()
            .filter(&is_active_on_current_platform)
            .filter(|link| visited.contains(link.to().name()))
            .map(|link| link.to().name().to_string())
            .collect();
        result.push(CrateTopo {
            name: pkg_name.clone(),
            deps,
            external: !workspace_names.contains(pkg_name.as_str()),
        });
    }

    log::debug!(
        "extract_graph total took {:?} ({} crates)",
        t.elapsed(),
        result.len()
    );
    result
}

// ── PTY helpers ──────────────────────────────────────────────────────────────

/// Create a pseudo-terminal pair, returning (controller, user) file descriptors.
fn open_pty() -> std::io::Result<(OwnedFd, OwnedFd)> {
    let mut controller: libc::c_int = 0;
    let mut user: libc::c_int = 0;
    if unsafe {
        libc::openpty(
            &mut controller,
            &mut user,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error());
    }
    unsafe { Ok((OwnedFd::from_raw_fd(controller), OwnedFd::from_raw_fd(user))) }
}

/// Copy the terminal window size from one fd to another so that cargo's
/// progress bar renders at the correct width.
fn copy_winsize(from: &OwnedFd, to: &OwnedFd) {
    unsafe {
        let mut ws: libc::winsize = std::mem::zeroed();
        if libc::ioctl(from.as_raw_fd(), libc::TIOCGWINSZ, &mut ws) == 0 {
            libc::ioctl(to.as_raw_fd(), libc::TIOCSWINSZ, &ws);
        }
    }
}

// Global fd used by the SIGWINCH handler to propagate terminal resizes.
static PTY_CTL_FD: AtomicI32 = AtomicI32::new(-1);
static STDERR_FD: AtomicI32 = AtomicI32::new(-1);

extern "C" fn handle_sigwinch(_sig: libc::c_int) {
    let from = STDERR_FD.load(Ordering::Relaxed);
    let to = PTY_CTL_FD.load(Ordering::Relaxed);
    if from >= 0 && to >= 0 {
        unsafe {
            let mut ws: libc::winsize = std::mem::zeroed();
            if libc::ioctl(from, libc::TIOCGWINSZ, &mut ws) == 0 {
                libc::ioctl(to, libc::TIOCSWINSZ, &ws);
            }
        }
    }
}

fn install_sigwinch_handler(stderr_fd: i32, pty_ctl_fd: i32) {
    STDERR_FD.store(stderr_fd, Ordering::Relaxed);
    PTY_CTL_FD.store(pty_ctl_fd, Ordering::Relaxed);
    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = handle_sigwinch as *const () as usize;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigaction(libc::SIGWINCH, &sa, std::ptr::null_mut());
    }
}

fn run_cargo_json_mode(
    cargo: &Path,
    args: &[String],
) -> anyhow::Result<(std::process::ExitStatus, Vec<String>)> {
    let mut cmd = std::process::Command::new(cargo);
    cmd.args(args);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::inherit());

    let mut child = cmd.spawn()?;
    let mut out = child.stdout.take().expect("stdout was piped");

    let mut dirty_crates = Vec::new();
    let mut seen_dirty = HashSet::new();
    let mut line_buf = Vec::new();
    let mut out_err = std::io::stderr();
    let mut buf = [0u8; 8192];

    loop {
        let n = match out.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };

        let chunk = &buf[..n];
        let _ = out_err.write_all(chunk);

        for &byte in chunk {
            if byte == b'\n' || byte == b'\r' {
                if !line_buf.is_empty()
                    && let Ok(line) = std::str::from_utf8(&line_buf)
                    && let Some(name) = parse_json_message_dirty_crate(line)
                    && seen_dirty.insert(name.clone())
                {
                    dirty_crates.push(name);
                }
                line_buf.clear();
            } else {
                line_buf.push(byte);
            }
        }
    }

    if !line_buf.is_empty()
        && let Ok(line) = std::str::from_utf8(&line_buf)
        && let Some(name) = parse_json_message_dirty_crate(line)
        && seen_dirty.insert(name.clone())
    {
        dirty_crates.push(name);
    }

    let status = child.wait()?;
    Ok((status, dirty_crates))
}

/// Spawn cargo, forward stderr to the real stderr, and collect the names
/// of crates that were (re)compiled.
///
/// In JSON mode, cargo emits machine-readable messages on stdout, so we parse
/// dirty crates from stdout while still inheriting stderr.
/// In human mode, we keep the PTY/pipe stderr tee behavior for line parsing.
fn run_cargo_tee(
    cargo: &Path,
    args: &[String],
    message_format: MessageFormat,
) -> anyhow::Result<(std::process::ExitStatus, Vec<String>)> {
    if matches!(message_format, MessageFormat::Json) {
        return run_cargo_json_mode(cargo, args);
    }

    let real_stderr = std::io::stderr();
    let is_tty = real_stderr.is_terminal();

    let mut cmd = std::process::Command::new(cargo);
    cmd.args(args);
    cmd.stdout(Stdio::inherit());

    // Set up the child's stderr: PTY when interactive, pipe otherwise.
    let pty_ctl: Option<OwnedFd> = if is_tty {
        let (controller, user_end) = open_pty()?;
        // Make the PTY the same size as the real terminal.
        let stderr_fd = unsafe { OwnedFd::from_raw_fd(real_stderr.as_raw_fd()) };
        copy_winsize(&stderr_fd, &controller);

        // Forward future terminal resizes to the PTY.
        install_sigwinch_handler(stderr_fd.as_raw_fd(), controller.as_raw_fd());

        // Don't let this drop close the real stderr.
        std::mem::forget(stderr_fd);

        cmd.stderr(Stdio::from(user_end)); // consumed → closed in parent after spawn
        Some(controller)
    } else {
        cmd.stderr(Stdio::piped());
        None
    };

    let mut child = cmd.spawn()?;
    drop(cmd); // Close the parent's copy of the user fd so the controller gets EIO when the child exits.

    // Obtain the readable end of whichever mechanism we chose.
    let mut reader: Box<dyn Read> = if let Some(ctl) = pty_ctl {
        Box::new(std::fs::File::from(ctl))
    } else {
        Box::new(child.stderr.take().expect("stderr was piped"))
    };

    let mut dirty_crates = Vec::new();
    let mut seen_dirty = HashSet::new();
    let mut err = std::io::stderr();
    let mut line_buf = Vec::new();
    let mut buf = [0u8; 8192];

    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            // PTY controller returns EIO once the user side is closed.
            Err(e) if e.raw_os_error() == Some(libc::EIO) => break,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        };

        let chunk = &buf[..n];
        let _ = err.write_all(chunk);

        // Accumulate bytes and extract lines (split on \n or \r) for parsing.
        for &byte in chunk {
            if byte == b'\n' || byte == b'\r' {
                if !line_buf.is_empty()
                    && let Ok(line) = std::str::from_utf8(&line_buf)
                    && let Some(name) = parse_compiling_line(line)
                    && seen_dirty.insert(name.clone())
                {
                    dirty_crates.push(name);
                }
                line_buf.clear();
            } else {
                line_buf.push(byte);
            }
        }
    }

    if !line_buf.is_empty()
        && let Ok(line) = std::str::from_utf8(&line_buf)
        && let Some(name) = parse_compiling_line(line)
        && seen_dirty.insert(name.clone())
    {
        dirty_crates.push(name);
    }

    let status = child.wait()?;
    Ok((status, dirty_crates))
}

/// Write the pheromone output to the path in `$PHEROMONE_OUT`.
fn emit_output(parsed: &ParsedArgs, dirty_crates: Vec<String>, graph: Vec<CrateTopo>) {
    let Some(out_path) = std::env::var_os("PHEROMONE_OUT") else {
        return;
    };

    let output = PheromoneOutput {
        tool: "cargo".into(),
        command: parsed.command.clone(),
        platform: PheromoneOutput::detect_platform(),
        profile: Some(parsed.profile),
        packages: parsed.packages.clone(),
        dirty_crates,
        graph,
        extra: serde_json::Value::Null,
    };

    if let Ok(json) = serde_json::to_string(&output) {
        let _ = std::fs::write(out_path, json);
    }
}

fn run() -> anyhow::Result<ExitCode> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let parsed = parse_args(&args);
    let cargo = find_real_cargo()?;

    // For non-build commands just pass through, no event emitted.
    let Some(parsed) = parsed else {
        let status = std::process::Command::new(&cargo).args(&args).status()?;
        let code = status.code().unwrap_or(1) as u8;
        return Ok(ExitCode::from(code));
    };

    let message_format = detect_message_format(&args);
    let (status, dirty_crates) = run_cargo_tee(&cargo, &args, message_format)?;
    let graph = extract_graph(&cargo);

    emit_output(&parsed, dirty_crates, graph);

    let code = status.code().unwrap_or(1) as u8;
    Ok(ExitCode::from(code))
}

fn main() -> ExitCode {
    env_logger::init();
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("pheromone-cargo: {e}");
            ExitCode::from(127)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_build() {
        let args: Vec<String> = vec!["build".into()];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.command, "build");
        assert_eq!(parsed.profile, Profile::Dev);
        assert!(parsed.packages.is_empty());
    }

    #[test]
    fn parse_short_alias() {
        let args: Vec<String> = vec!["b".into()];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.command, "build");
    }

    #[test]
    fn parse_release_profile() {
        let args: Vec<String> = vec!["build".into(), "--release".into()];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.profile, Profile::Release);
    }

    #[test]
    fn parse_named_profile() {
        let args: Vec<String> = vec!["build".into(), "--profile".into(), "release".into()];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.profile, Profile::Release);
    }

    #[test]
    fn parse_named_profile_eq() {
        let args: Vec<String> = vec!["build".into(), "--profile=bench".into()];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.profile, Profile::Release);
    }

    #[test]
    fn parse_packages() {
        let args: Vec<String> = vec![
            "test".into(),
            "-p".into(),
            "foo".into(),
            "--package".into(),
            "bar".into(),
        ];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.command, "test");
        assert_eq!(parsed.packages, vec!["foo", "bar"]);
    }

    #[test]
    fn parse_package_eq() {
        let args: Vec<String> = vec!["check".into(), "--package=baz".into()];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.packages, vec!["baz"]);
    }

    #[test]
    fn parse_non_build_returns_none() {
        let args: Vec<String> = vec!["fmt".into()];
        assert!(parse_args(&args).is_none());
    }

    #[test]
    fn parse_global_flags_skipped() {
        let args: Vec<String> = vec![
            "--manifest-path".into(),
            "foo/Cargo.toml".into(),
            "build".into(),
            "--release".into(),
        ];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.command, "build");
        assert_eq!(parsed.profile, Profile::Release);
    }

    #[test]
    fn parse_empty_args() {
        let args: Vec<String> = vec![];
        assert!(parse_args(&args).is_none());
    }

    #[test]
    fn compiling_line_parsed() {
        let line = "   Compiling serde v1.0.228 (/some/path)";
        assert_eq!(parse_compiling_line(line), Some("serde".into()));
    }

    #[test]
    fn compiling_line_with_ansi() {
        let line = "\x1b[0m\x1b[0;32m   Compiling\x1b[0m serde v1.0.228 (/some/path)";
        assert_eq!(parse_compiling_line(line), Some("serde".into()));
    }

    #[test]
    fn compiling_line_no_match() {
        assert_eq!(parse_compiling_line("   Downloading foo v1.0"), None);
        assert_eq!(parse_compiling_line("   Fresh bar v0.1.0"), None);
        assert_eq!(parse_compiling_line("warning: unused variable"), None);
    }

    #[test]
    fn strip_ansi_plain() {
        assert_eq!(strip_ansi("hello world"), "hello world");
    }

    #[test]
    fn strip_ansi_colored() {
        assert_eq!(
            strip_ansi("\x1b[0;32mCompiling\x1b[0m foo"),
            "Compiling foo"
        );
    }

    #[test]
    fn parse_install_defaults_release() {
        let args: Vec<String> = vec!["install".into(), "ripgrep".into()];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.command, "install");
        assert_eq!(parsed.profile, Profile::Release);
    }

    #[test]
    fn parse_install_debug_flag() {
        let args: Vec<String> = vec!["install".into(), "--debug".into(), "ripgrep".into()];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.command, "install");
        assert_eq!(parsed.profile, Profile::Dev);
    }

    #[test]
    fn parse_install_explicit_profile() {
        let args: Vec<String> = vec!["install".into(), "--profile=dev".into(), "ripgrep".into()];
        let parsed = parse_args(&args).unwrap();
        assert_eq!(parsed.command, "install");
        assert_eq!(parsed.profile, Profile::Dev);
    }

    #[test]
    fn detect_message_format_human_default() {
        let args: Vec<String> = vec!["build".into()];
        assert_eq!(detect_message_format(&args), MessageFormat::Human);
    }

    #[test]
    fn detect_message_format_json_eq() {
        let args: Vec<String> = vec!["build".into(), "--message-format=json".into()];
        assert_eq!(detect_message_format(&args), MessageFormat::Json);
    }

    #[test]
    fn detect_message_format_json_split() {
        let args: Vec<String> = vec![
            "build".into(),
            "--message-format".into(),
            "json-diagnostic-rendered-ansi,json".into(),
        ];
        assert_eq!(detect_message_format(&args), MessageFormat::Json);
    }

    #[test]
    fn parse_json_compiler_artifact_dirty_crate() {
        let line = r#"{"reason":"compiler-artifact","package_id":"path+file:///tmp/ws/foo#foo@0.1.0","target":{"name":"foo","kind":["lib"],"crate_types":["lib"]}}"#;
        assert_eq!(
            parse_json_message_dirty_crate(line),
            Some("foo".to_string())
        );
    }

    #[test]
    fn parse_json_build_script_executed_dirty_crate() {
        let line = r#"{"reason":"build-script-executed","package_id":"registry+https://github.com/rust-lang/crates.io-index#serde_json@1.0.133"}"#;
        assert_eq!(
            parse_json_message_dirty_crate(line),
            Some("serde_json".to_string())
        );
    }

    #[test]
    fn parse_json_non_dirty_reason() {
        let line = r#"{"reason":"build-finished","success":true}"#;
        assert_eq!(parse_json_message_dirty_crate(line), None);
    }

    #[test]
    fn parse_json_from_stdout_chunking_simulation() {
        let lines = [
            r#"{"reason":"compiler-artifact","package_id":"path+file:///tmp/ws/foo#foo@0.1.0"}"#,
            r#"{"reason":"build-script-executed","package_id":"registry+https://github.com/rust-lang/crates.io-index#serde_json@1.0.133"}"#,
            r#"{"reason":"build-finished","success":true}"#,
        ];

        let mut dirty = Vec::new();
        let mut seen = HashSet::new();
        let mut buf = Vec::new();

        let stream = format!("{}\n{}\n{}\n", lines[0], lines[1], lines[2]);
        for b in stream.as_bytes() {
            if *b == b'\n' || *b == b'\r' {
                if !buf.is_empty()
                    && let Ok(line) = std::str::from_utf8(&buf)
                    && let Some(name) = parse_json_message_dirty_crate(line)
                    && seen.insert(name.clone())
                {
                    dirty.push(name);
                }
                buf.clear();
            } else {
                buf.push(*b);
            }
        }

        assert_eq!(dirty, vec!["foo".to_string(), "serde_json".to_string()]);
    }
}
