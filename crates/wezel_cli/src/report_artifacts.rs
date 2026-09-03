//! Write the report artifacts produced by `wezel experiment run`.
//!
//! This is deliberately local-only. Sabo owns all Burrow callbacks and any
//! transport-specific packaging; the CLI just materializes `report.json` plus
//! attachments in a directory.

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result, bail};
use wezel_bench::run::CompletedRunAttachment;
use wezel_types::ExperimentRunReport;

pub fn write_report_dir(
    dir: &Path,
    report: &ExperimentRunReport,
    attachments: &[CompletedRunAttachment],
) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;

    let report_path = dir.join("report.json");
    let report_bytes = serde_json::to_vec_pretty(report).context("serializing report.json")?;
    std::fs::write(&report_path, report_bytes)
        .with_context(|| format!("writing {}", report_path.display()))?;

    let mut seen = HashSet::new();
    for attachment in attachments {
        if !seen.insert(attachment.archive_path.as_str()) {
            bail!(
                "duplicate report attachment path {}",
                attachment.archive_path
            );
        }
        let target = dir.join(&attachment.archive_path);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::copy(&attachment.source_path, &target).with_context(|| {
            format!(
                "copying attachment {} to {}",
                attachment.source_path.display(),
                target.display()
            )
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_dir_contains_manifest_and_attachments() {
        let dir =
            std::env::temp_dir().join(format!("wezel-report-dir-test-{}", uuid::Uuid::new_v4()));
        let source_dir = dir.join("source");
        std::fs::create_dir_all(&source_dir).unwrap();
        let source_path = source_dir.join("log.txt");
        std::fs::write(&source_path, b"hello").unwrap();
        let report_dir = dir.join("report");

        write_report_dir(
            &report_dir,
            &ExperimentRunReport {
                run_id: 7,
                steps: Vec::new(),
                summaries: Vec::new(),
            },
            &[CompletedRunAttachment {
                archive_path: "attachments/000-build/0/000-log.txt".into(),
                source_path,
            }],
        )
        .unwrap();

        assert!(report_dir.join("report.json").is_file());
        assert_eq!(
            std::fs::read(report_dir.join("attachments/000-build/0/000-log.txt")).unwrap(),
            b"hello"
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
