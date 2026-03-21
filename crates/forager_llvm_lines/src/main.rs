use anyhow::{Context, Result, bail};
use wezel_types::{ForagerPluginEnvelope, ForagerPluginOutput, MeasurementDetail};

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
    let inputs: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&inputs_path)?)
            .context("parsing FORAGER_INPUTS")?;
    let package = inputs.get("package").and_then(|v| v.as_str());

    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("llvm-lines");
    if let Some(pkg) = package {
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
/// Expected format (header + data rows):
/// ```
///   Lines         Copies       Function name
///   -----         ------       -------------
///   6207 (100%)   186 (100%)   (TOTAL)
///    405  (6.5%)    1  (0.5%)  std::rt::lang_start_internal
///   ...
/// ```
fn parse_llvm_lines_output(s: &str) -> Result<(u64, Vec<MeasurementDetail>)> {
    let mut total: u64 = 0;
    let mut detail: Vec<MeasurementDetail> = Vec::new();

    for line in s.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty()
            || trimmed.starts_with("Lines")
            || trimmed.starts_with("-----")
        {
            continue;
        }

        // Each data line: <count> (<pct>%)  <copies> (<pct>%)  <name>
        let mut parts = trimmed.splitn(5, char::is_whitespace).filter(|s| !s.is_empty());
        let Some(count_str) = parts.next() else { continue };
        let count: u64 = count_str.parse().unwrap_or(0);

        // Skip the percentage field "(X.X%)"
        let _ = parts.next();
        // Skip copies field
        let _ = parts.next();
        // Skip copies percentage
        let _ = parts.next();
        // Rest is the function/item name
        let name = trimmed
            .splitn(5, char::is_whitespace)
            .filter(|s| !s.is_empty())
            .nth(4)
            .unwrap_or("")
            .trim()
            .to_string();

        if name == "(TOTAL)" {
            total = count;
        } else if !name.is_empty() && detail.len() < 50 {
            detail.push(MeasurementDetail {
                name,
                value: count as f64,
                prev_value: None,
            });
        }
    }

    Ok((total, detail))
}
