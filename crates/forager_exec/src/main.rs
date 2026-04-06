use std::collections::HashMap;
use std::process;

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Deserialize)]
struct ExecInputs {
    cmd: String,
    #[serde(default)]
    env: HashMap<String, String>,
    cwd: Option<String>,
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

    let inputs: ExecInputs = serde_json::from_str(
        &std::fs::read_to_string(&inputs_path).with_context(|| format!("reading {inputs_path}"))?,
    )
    .context("parsing FORAGER_INPUTS")?;

    let mut child = process::Command::new("sh");
    child.arg("-c").arg(&inputs.cmd);
    for (k, v) in &inputs.env {
        child.env(k, v);
    }
    if let Some(dir) = &inputs.cwd {
        child.current_dir(dir);
    }

    // Write the empty measurements output before running (exec produces no measurements).
    let envelope = wezel_types::ForagerPluginEnvelope {
        measurements: vec![],
    };
    std::fs::write(&out_path, serde_json::to_string(&envelope)?)
        .with_context(|| format!("writing {out_path}"))?;

    let status = child.status().context("failed to spawn command")?;
    process::exit(status.code().unwrap_or(1));
}
