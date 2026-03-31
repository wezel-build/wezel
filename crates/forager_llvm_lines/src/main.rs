use anyhow::{Context, Result, bail};
use serde::Deserialize;
use wezel_types::{ForagerPluginEnvelope, ForagerPluginOutput, MeasurementDetail};

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
                    "kind": "count",
                    "unit": "lines",
                    "description": "Total LLVM IR lines; detail is top functions by line count"
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
    let (total, detail) = parse_llvm_lines_output(&stdout)?;

    let measurement = ForagerPluginOutput {
        name: "llvm-lines".to_string(),
        kind: "count".to_string(),
        value: total as f64,
        unit: Some("lines".to_string()),
        detail,
    };

    let envelope = ForagerPluginEnvelope {
        measurement: Some(measurement),
    };

    std::fs::write(&out_path, serde_json::to_string(&envelope)?)
        .with_context(|| format!("writing {out_path}"))?;

    Ok(())
}

/// Parse `cargo llvm-lines` output.
///
/// Format:
/// ```text
///   Lines                 Copies               Function name
///   -----                 ------               -------------
///   361539                12556                (TOTAL)
///     6639 (1.8%,  1.8%)     22 (0.2%,  0.2%)  <F as axum::handler::Handler<...>>::call
/// ```
///
/// The TOTAL line is the first line after the `---` separator.
/// Detail rows have: lines (pct, pct) copies (pct, pct) name
fn parse_llvm_lines_output(s: &str) -> Result<(u64, Vec<MeasurementDetail>)> {
    let mut lines = s.lines();

    // Skip until the --- separator line.
    loop {
        match lines.next() {
            None => bail!("unexpected end of cargo llvm-lines output: no separator line"),
            Some(line) if line.trim().starts_with("-----") => break,
            Some(_) => continue,
        }
    }

    // Next line is the TOTAL row: <lines> <copies> (TOTAL)
    let total_line = lines.next().context("no TOTAL line after separator")?;
    let total_parts: Vec<&str> = total_line.split_whitespace().collect();
    // total_parts: ["361539", "12556", "(TOTAL)"]
    let total: u64 = total_parts
        .first()
        .and_then(|s| s.parse().ok())
        .context("could not parse total line count")?;

    // Remaining lines are detail rows.
    let mut detail = Vec::new();
    for line in lines {
        if detail.len() >= 50 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        // First token is the line count.
        let count_str = trimmed.split_whitespace().next().unwrap_or("");
        let Ok(count) = count_str.parse::<u64>() else {
            continue;
        };
        // Function name is the last whitespace-delimited "word" that doesn't
        // look like a number or percentage. Find it after the last ')'.
        let name = match trimmed.rfind(')') {
            Some(pos) => trimmed[pos + 1..].trim(),
            None => continue,
        };
        if !name.is_empty() {
            detail.push(MeasurementDetail {
                name: name.to_string(),
                value: count as f64,
            });
        }
    }

    Ok((total, detail))
}
