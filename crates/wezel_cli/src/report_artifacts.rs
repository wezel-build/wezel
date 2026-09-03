//! Write the report artifacts produced by `wezel experiment run`.
//!
//! This is deliberately local-only. Sabo owns all Burrow callbacks and any
//! transport-specific packaging; the CLI just materializes `report.json` in the
//! same saved run directory that already contains attachments.

use std::path::Path;

use anyhow::{Context, Result};
use wezel_types::ExperimentRunReport;

pub fn write_report_json(dir: &Path, report: &ExperimentRunReport) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    let report_path = dir.join("report.json");
    let report_bytes = serde_json::to_vec_pretty(report).context("serializing report.json")?;
    std::fs::write(&report_path, report_bytes)
        .with_context(|| format!("writing {}", report_path.display()))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_report_json() {
        let dir =
            std::env::temp_dir().join(format!("wezel-report-json-test-{}", uuid::Uuid::new_v4()));

        write_report_json(
            &dir,
            &ExperimentRunReport {
                run_id: 7,
                steps: Vec::new(),
                summaries: Vec::new(),
            },
        )
        .unwrap();

        let report: ExperimentRunReport =
            serde_json::from_slice(&std::fs::read(dir.join("report.json")).unwrap()).unwrap();
        assert_eq!(report.run_id, 7);
        let _ = std::fs::remove_dir_all(dir);
    }
}
