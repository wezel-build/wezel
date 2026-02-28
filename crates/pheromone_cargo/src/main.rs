use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{ExitCode, Stdio};

use pheromone_types::{CrateTopo, PheromoneOutput, Profile};

/// Cargo subcommands that trigger a build.
const BUILD_COMMANDS: &[&str] = &[
    "build", "b", "check", "c", "test", "t", "bench", "run", "r", "clippy", "doc", "d", "rustc",
    "rustdoc",
];

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
    let mut profile = Profile::Dev;
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
            "--release" => profile = Profile::Release,
            "--profile" => {
                if let Some(val) = iter.next() {
                    profile = match val.as_str() {
                        "release" | "bench" => Profile::Release,
                        _ => Profile::Dev,
                    };
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
                    profile = match val {
                        "release" | "bench" => Profile::Release,
                        _ => Profile::Dev,
                    };
                }
            }
            _ => {}
        }
    }

    Some(ParsedArgs {
        command: normalize_command(&command).to_string(),
        profile,
        packages,
    })
}

/// Extract a crate name from a cargo stderr line like:
///   `   Compiling foo v0.1.0 (/path/to/foo)`
fn parse_compiling_line(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("Compiling ")?;
    let name = rest.split_whitespace().next()?;
    Some(name.to_string())
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
fn extract_graph(cargo: &PathBuf) -> Vec<CrateTopo> {
    let graph = match guppy::MetadataCommand::new()
        .cargo_path(cargo)
        .build_graph()
    {
        Ok(g) => g,
        Err(_) => return Vec::new(),
    };

    graph
        .workspace()
        .iter()
        .map(|pkg| {
            let deps: Vec<String> = pkg
                .direct_links()
                .map(|link| link.to().name().to_string())
                .collect();
            CrateTopo {
                name: pkg.name().to_string(),
                deps,
            }
        })
        .collect()
}

/// Spawn cargo with stderr piped. Forward every line to real stderr while
/// collecting the names of crates that were (re)compiled.
fn run_cargo_tee(
    cargo: &PathBuf,
    args: &[String],
) -> anyhow::Result<(std::process::ExitStatus, Vec<String>)> {
    let mut child = std::process::Command::new(cargo)
        .args(args)
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()?;

    let stderr_pipe = child.stderr.take().expect("stderr was piped");
    let reader = BufReader::new(stderr_pipe);
    let mut dirty_crates = Vec::new();
    let real_stderr = std::io::stderr();

    for line in reader.lines() {
        let line = line?;
        {
            let mut err = real_stderr.lock();
            let _ = writeln!(err, "{}", line);
        }
        if let Some(name) = parse_compiling_line(&line) {
            dirty_crates.push(name);
        }
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

    // Collect the workspace graph before the build. This is cheap (no compilation)
    // and gives us the topology regardless of whether the build succeeds.
    let graph = extract_graph(&cargo);

    let (status, dirty_crates) = run_cargo_tee(&cargo, &args)?;

    emit_output(&parsed, dirty_crates, graph);

    let code = status.code().unwrap_or(1) as u8;
    Ok(ExitCode::from(code))
}

fn main() -> ExitCode {
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
    fn compiling_line_no_match() {
        assert_eq!(parse_compiling_line("   Downloading foo v1.0"), None);
        assert_eq!(parse_compiling_line("   Fresh bar v0.1.0"), None);
        assert_eq!(parse_compiling_line("warning: unused variable"), None);
    }
}
