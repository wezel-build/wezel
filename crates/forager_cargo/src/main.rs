use std::process;

use anyhow::{Context, Result};
use serde::Deserialize;
use wezel_types::ForagerPluginOutput;

#[derive(Deserialize)]
#[serde(untagged)]
enum PackageSpecifier {
    Packages(Vec<String>),
    Workspace,
}

#[derive(Default, Deserialize)]
enum Command {
    #[default]
    Build,
    Bench,
    Test,
}

impl Command {
    fn as_str(&self) -> &'static str {
        match self {
            Command::Build => "build",
            Command::Bench => "bench",
            Command::Test => "test",
        }
    }
}

#[derive(Deserialize)]
struct CargoInputs {
    command: Command,
    #[serde(flatten)]
    target: PackageSpecifier,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--schema") {
        println!(
            "{}",
            serde_json::json!({
                "name": "exec",
                "description": "Executes a shell command; produces no measurements",
                "inputs": {
                    "cmd": { "type": "string", "description": "Shell command to run" },
                    "env": { "type": "object", "description": "Extra environment variables" },
                    "cwd": { "type": "string", "description": "Working directory override" }
                },
                "output": null
            })
        );
        return Ok(());
    }

    let inputs_path = std::env::var("FORAGER_INPUTS").context("FORAGER_INPUTS not set")?;
    let out_path = std::env::var("FORAGER_OUT").context("FORAGER_OUT not set")?;

    let inputs: CargoInputs = serde_json::from_str(
        &std::fs::read_to_string(&inputs_path).with_context(|| format!("reading {inputs_path}"))?,
    )
    .context("parsing FORAGER_INPUTS")?;

    let mut child = process::Command::new("cargo");

    child.arg(inputs.command.as_str());

    match inputs.target {
        PackageSpecifier::Packages(items) => {
            items
                .into_iter()
                .fold(&mut child, |child, package| child.arg("-p").arg(package));
        }
        PackageSpecifier::Workspace => {
            child.arg("--workspace");
        }
    }

    let timer_start = std::time::Instant::now();
    child.spawn()?;
    let end = timer_start.elapsed();
    // Write the empty measurements output before running (exec produces no measurements).
    let value: u64 = end
        .as_millis()
        .try_into()
        .context("Could not stuff build duration into u64")?;
    let envelope = wezel_types::ForagerPluginEnvelope {
        measurements: vec![ForagerPluginOutput {
            name: "time_ms".to_owned(),
            value: value.into(),
            tags: Default::default(),
        }],
    };
    std::fs::write(&out_path, serde_json::to_string(&envelope)?)
        .with_context(|| format!("writing {out_path}"))?;

    let status = child.status().context("failed to spawn command")?;
    process::exit(status.code().unwrap_or(1));
}
