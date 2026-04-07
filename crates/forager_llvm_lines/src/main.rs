use anyhow::{Context, Result, bail};
use serde::Deserialize;
use wezel_types::{ForagerPluginEnvelope, ForagerPluginOutput};

#[derive(Deserialize)]
struct LlvmLinesInputs {
    package: Option<String>,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--schema") {
        println!(
            "{}",
            serde_json::json!({
                "name": "llvm-lines",
                "description": "Counts LLVM IR lines via cargo-llvm-lines",
                "inputs": {
                    "package": { "type": "string", "description": "Package name (required for workspaces)", "optional": true }
                },
                "output": {
                    "description": "One `llvm-lines` measurement per function per unit (`lines` or `copies`), tagged with `function` and `unit`. Untagged totals are also emitted."
                }
            })
        );
        return Ok(());
    }

    let out_path = std::env::var("FORAGER_OUT").context("FORAGER_OUT not set")?;
    let inputs_path = std::env::var("FORAGER_INPUTS").context("FORAGER_INPUTS not set")?;
    let inputs: LlvmLinesInputs = serde_json::from_str(
        &std::fs::read_to_string(&inputs_path).with_context(|| format!("reading {inputs_path}"))?,
    )
    .context("parsing FORAGER_INPUTS")?;

    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("llvm-lines");
    if let Some(pkg) = &inputs.package {
        cmd.args(["-p", pkg]);
    }
    let output = cmd.output().context("failed to run cargo llvm-lines")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("cargo llvm-lines failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let (total_lines, total_copies, functions) = parse_llvm_lines_output(&stdout)?;

    let mut measurements = Vec::with_capacity(2 + functions.len() * 2);

    // Untagged totals for regression detection.
    measurements.push(m(total_lines, &[]));
    measurements.push(m(total_copies, &[("unit", "copies")]));

    for (fn_name, lines, copies) in &functions {
        measurements.push(m(*lines, &[("function", fn_name), ("unit", "lines")]));
        measurements.push(m(*copies, &[("function", fn_name), ("unit", "copies")]));
    }

    let envelope = ForagerPluginEnvelope { measurements };
    std::fs::write(&out_path, serde_json::to_string(&envelope)?)
        .with_context(|| format!("writing {out_path}"))?;

    Ok(())
}

fn m(value: u64, tags: &[(&str, &str)]) -> ForagerPluginOutput {
    ForagerPluginOutput {
        name: "llvm-lines".to_string(),
        value: serde_json::json!(value),
        tags: tags
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    }
}

/// Parse `cargo llvm-lines` output.
///
/// Format:
/// ```text
///   Lines                 Copies               Function name
///   -----                 ------               -------------
///   361539                12556                (TOTAL)
///     6639 (1.8%,  1.8%)     22 (0.2%,  0.2%)  core::ops::function::FnOnce::call_once
/// ```
///
/// Returns `(total_lines, total_copies, Vec<(name, lines, copies)>)`.
fn parse_llvm_lines_output(s: &str) -> Result<(u64, u64, Vec<(String, u64, u64)>)> {
    let mut lines = s.lines();

    loop {
        match lines.next() {
            None => bail!("unexpected end of cargo llvm-lines output: no separator line"),
            Some(line) if line.trim().starts_with("-----") => break,
            Some(_) => continue,
        }
    }

    // TOTAL row: "<total_lines>  <total_copies>  (TOTAL)"
    let total_line = lines.next().context("no TOTAL line after separator")?;
    let mut total_parts = total_line.split_whitespace();
    let total_lines: u64 = total_parts
        .next()
        .and_then(|s| s.parse().ok())
        .context("could not parse total line count")?;
    let total_copies: u64 = total_parts
        .next()
        .and_then(|s| s.parse().ok())
        .context("could not parse total copies count")?;

    // Per-function rows: "<lines> (<pct>, <pct>)  <copies> (<pct>, <pct>)  <name>"
    let mut functions = Vec::new();
    for line in lines {
        if functions.len() >= 50 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let fn_lines: u64 = match trimmed.split_whitespace().next().and_then(|s| s.parse().ok()) {
            Some(n) => n,
            None => continue,
        };
        let fn_copies: u64 = match trimmed.find(')').and_then(|pos| {
            trimmed[pos + 1..]
                .split_whitespace()
                .find_map(|t| t.parse().ok())
        }) {
            Some(n) => n,
            None => continue,
        };
        let name = match trimmed.rfind(')') {
            Some(pos) => trimmed[pos + 1..].trim(),
            None => continue,
        };
        if !name.is_empty() {
            functions.push((name.to_string(), fn_lines, fn_copies));
        }
    }

    Ok((total_lines, total_copies, functions))
}
