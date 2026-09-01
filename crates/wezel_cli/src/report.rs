//! Human-readable rendering of `wezel experiment run`.
//!
//! Layout: an identity line, the baseline being compared against, per-step
//! sample values for sampled steps, then the summary table with this run,
//! the previous run, and the delta.

use std::fmt::Write as _;
use std::path::Path;
use std::time::Duration;

use indexmap::IndexMap;
use owo_colors::{OwoColorize, Stream::Stdout};
use wezel_bench::run::{SavedRun, StepPlan};
use wezel_types::{MetricDirection, SummaryDef, Unit};

/// One rendered summary table row. Numeric cells are pre-formatted strings so
/// column widths can be measured before any colour codes are applied.
struct Row {
    name: String,
    this_run: String,
    previous: String,
    delta: String,
    percent: String,
    /// `Some(true)` = regression, `Some(false)` = improvement, `None` = no
    /// change or no baseline to compare against.
    worse: Option<bool>,
}

/// Print the report for a finished run to stdout.
pub fn print_run(
    saved: &SavedRun,
    plan: &[StepPlan],
    summary_defs: &[SummaryDef],
    measuring: &IndexMap<String, Duration>,
    previous: Option<&SavedRun>,
    verbose: bool,
) {
    print!(
        "{}",
        render_run(saved, plan, summary_defs, measuring, previous, verbose)
    );
}

/// Dim trailer naming where the run was saved, relative to the project dir
/// when it sits inside it.
pub fn print_saved_at(run_dir: &Path, project_dir: &Path) {
    let shown = run_dir.strip_prefix(project_dir).unwrap_or(run_dir);
    println!(
        "\n{}",
        format!("saved {}", shown.display()).if_supports_color(Stdout, |t| t.dimmed())
    );
}

/// Render the report as text. Colour is applied only when stdout supports it,
/// so this is plain text under a pipe or in tests.
fn render_run(
    saved: &SavedRun,
    plan: &[StepPlan],
    summary_defs: &[SummaryDef],
    measuring: &IndexMap<String, Duration>,
    previous: Option<&SavedRun>,
    verbose: bool,
) -> String {
    let mut out = String::new();
    let output = &saved.output;
    let short = short_sha(&output.commit);
    let branch = saved.branch.as_deref().unwrap_or("(detached)");
    let dirty = if saved.dirty { " (dirty)" } else { "" };
    let duration = format_millis(saved.duration_ms as f64);

    let _ = writeln!(
        out,
        "{} {}",
        output.experiment.if_supports_color(Stdout, |t| t.bold()),
        format!("· {short} on {branch}{dirty} · {duration}")
            .if_supports_color(Stdout, |t| t.dimmed())
    );
    let _ = writeln!(
        out,
        "{}",
        baseline_line(previous).if_supports_color(Stdout, |t| t.dimmed())
    );

    render_steps(&mut out, saved, plan, summary_defs);

    if output.summaries.is_empty() {
        let _ = writeln!(out, "\n(no summaries)");
    } else {
        render_table(&mut out, saved, summary_defs, previous);
    }

    if verbose {
        render_outcomes(&mut out, saved, measuring);
    }
    out
}

/// `against 8f21c4b · previous run, 6 days ago`, or a note that there is
/// nothing to compare against yet.
fn baseline_line(previous: Option<&SavedRun>) -> String {
    let Some(prev) = previous else {
        return "no previous run to compare against".to_string();
    };
    let short = short_sha(&prev.output.commit);
    let dirty = if prev.dirty { " (dirty)" } else { "" };
    match relative_age(&prev.started_at) {
        Some(age) => format!("against {short}{dirty} · previous run, {age}"),
        None => format!("against {short}{dirty} · previous run"),
    }
}

/// Sample values for summaries that took more than one sample, grouped under
/// their step. Steps that produced a single value contribute nothing here —
/// that value is already in the table, and how long the step took to run isn't
/// what the experiment measures (see `-v` for wall-clock per step).
fn render_steps(
    out: &mut String,
    saved: &SavedRun,
    plan: &[StepPlan],
    summary_defs: &[SummaryDef],
) {
    let steps = &saved.output.steps;
    for step in plan {
        let sampled: Vec<_> = summary_defs
            .iter()
            .filter(|def| def.step == step.name)
            .map(|def| (def, def.matching_values(steps)))
            .filter(|(_, values)| values.len() > 1)
            .collect();
        if sampled.is_empty() {
            continue;
        }

        let _ = writeln!(
            out,
            "\n{}",
            format!("step.{}.{}", step.forager, step.name).if_supports_color(Stdout, |t| t.bold())
        );

        let name_w = sampled.iter().map(|(d, _)| d.name.len()).max().unwrap_or(0);
        for (def, values) in sampled {
            let unit = def.unit(steps);
            let rendered: Vec<String> = values.iter().map(|v| format_value(*v, unit)).collect();
            let _ = writeln!(
                out,
                "  {:<name_w$}  {}  {}",
                def.name,
                format!("{} samples", values.len()).if_supports_color(Stdout, |t| t.dimmed()),
                rendered.join("  ")
            );
        }
    }
}

fn render_table(
    out: &mut String,
    saved: &SavedRun,
    summary_defs: &[SummaryDef],
    previous: Option<&SavedRun>,
) {
    let steps = &saved.output.steps;
    let rows: Vec<Row> = saved
        .output
        .summaries
        .iter()
        .map(|(name, sv)| {
            let def = summary_defs.iter().find(|d| &d.name == name);
            let unit = def.and_then(|d| d.unit(steps));
            let direction = def.map(|d| d.direction(steps)).unwrap_or_default();
            let prev = previous
                .and_then(|p| p.output.summaries.get(name))
                .map(|s| s.value);

            let (previous_cell, delta, percent, worse) = match prev {
                None => ("—".to_string(), String::new(), String::new(), None),
                Some(prev) => {
                    let diff = sv.value - prev;
                    if diff.abs() < f64::EPSILON {
                        (
                            format_value(prev, unit),
                            "±0".to_string(),
                            String::new(),
                            None,
                        )
                    } else {
                        let worse = match direction {
                            MetricDirection::LowerIsBetter => diff > 0.0,
                            MetricDirection::HigherIsBetter => diff < 0.0,
                        };
                        (
                            format_value(prev, unit),
                            format_signed(diff, unit),
                            format_percent(diff, prev),
                            Some(worse),
                        )
                    }
                }
            };

            Row {
                name: name.clone(),
                this_run: format_value(sv.value, unit),
                previous: previous_cell,
                delta,
                percent,
                worse,
            }
        })
        .collect();

    let name_w = width(rows.iter().map(|r| r.name.as_str()), "SUMMARY");
    let this_w = width(rows.iter().map(|r| r.this_run.as_str()), "THIS RUN");
    let prev_w = width(rows.iter().map(|r| r.previous.as_str()), "PREVIOUS");
    let delta_w = width(rows.iter().map(|r| r.delta.as_str()), "Δ");
    let pct_w = rows.iter().map(|r| r.percent.len()).max().unwrap_or(0);

    let header = format!(
        "{:<name_w$}  {:>this_w$}  {:>prev_w$}  {:>delta_w$}  {:>pct_w$}",
        "SUMMARY", "THIS RUN", "PREVIOUS", "Δ", ""
    );
    let header = header.trim_end();

    // Pad every cell before colouring — escape codes would otherwise count as
    // width — and keep the plain text so the rule can span the widest line.
    let padded: Vec<_> = rows
        .iter()
        .map(|row| {
            let plain = format!(
                "{:<name_w$}  {:>this_w$}  {:>prev_w$}  {:>delta_w$}  {:>pct_w$}",
                row.name, row.this_run, row.previous, row.delta, row.percent
            );
            (
                row,
                format!("{:<name_w$}", row.name),
                format!("{:>this_w$}", row.this_run),
                format!("{:>prev_w$}", row.previous),
                format!("{:>delta_w$}", row.delta),
                format!("{:>pct_w$}", row.percent),
                plain.trim_end().chars().count(),
            )
        })
        .collect();

    let rule = padded
        .iter()
        .map(|(.., plain_width)| *plain_width)
        .chain(std::iter::once(header.chars().count()))
        .max()
        .unwrap_or(0);

    let _ = writeln!(
        out,
        "\n{}",
        header.if_supports_color(Stdout, |t| t.dimmed())
    );
    let _ = writeln!(
        out,
        "{}",
        "─".repeat(rule).if_supports_color(Stdout, |t| t.dimmed())
    );

    for (row, name, this_run, previous, delta, percent) in padded
        .iter()
        .map(|(r, n, t, p, d, pc, _)| (r, n, t, p, d, pc))
    {
        let (delta, percent) = match row.worse {
            Some(true) => (
                delta.if_supports_color(Stdout, |t| t.red()).to_string(),
                percent.if_supports_color(Stdout, |t| t.red()).to_string(),
            ),
            Some(false) => (
                delta.if_supports_color(Stdout, |t| t.green()).to_string(),
                percent.if_supports_color(Stdout, |t| t.green()).to_string(),
            ),
            None => (
                delta.if_supports_color(Stdout, |t| t.dimmed()).to_string(),
                percent.clone(),
            ),
        };
        let line = format!(
            "{name}  {this_run}  {}  {delta}  {percent}",
            previous.if_supports_color(Stdout, |t| t.dimmed())
        );
        let _ = writeln!(out, "{}", line.trim_end());
    }
}

/// Raw per-step dump behind `-v`: how long each step spent in its forager, and
/// the outcomes exactly as the forager emitted them, tags included. Debugging
/// aid for experiment definitions, hence no unit formatting.
fn render_outcomes(out: &mut String, saved: &SavedRun, measuring: &IndexMap<String, Duration>) {
    let _ = writeln!(out, "\nOutcomes:");
    for report in &saved.output.steps {
        let elapsed = measuring
            .get(&report.step)
            .map(|d| format!("  {}", format_dur(*d)))
            .unwrap_or_default();
        let _ = writeln!(
            out,
            "  {}{}",
            report.step,
            elapsed.if_supports_color(Stdout, |t| t.dimmed())
        );
        if report.measurements.is_empty() {
            let _ = writeln!(out, "    (no outcomes)");
            continue;
        }
        for m in &report.measurements {
            let mut line = format!("    {} = {}", m.name, m.value);
            if !m.tags.is_empty() {
                let mut tags: Vec<_> = m.tags.iter().collect();
                tags.sort_by(|a, b| a.0.cmp(b.0));
                let joined: Vec<_> = tags.iter().map(|(k, v)| format!("{k}={v}")).collect();
                line.push_str(&format!(" [{}]", joined.join(", ")));
            }
            let _ = writeln!(out, "{line}");
        }
    }
}

fn width<'a>(cells: impl Iterator<Item = &'a str>, header: &str) -> usize {
    cells
        .map(str::chars)
        .map(Iterator::count)
        .chain(std::iter::once(header.chars().count()))
        .max()
        .unwrap_or(0)
}

fn short_sha(sha: &str) -> &str {
    &sha[..7.min(sha.len())]
}

// ── Value formatting ─────────────────────────────────────────────────────────

/// Render `value` in `unit`. Unitless values print as plain numbers.
fn format_value(value: f64, unit: Option<Unit>) -> String {
    match unit {
        Some(Unit::Milliseconds) => format_millis(value),
        Some(Unit::Bytes) => format_bytes(value),
        Some(Unit::Count) => format_count(value),
        None => format_plain(value),
    }
}

/// Render a delta with an explicit sign, e.g. `+12.4s`, `-310 k`.
fn format_signed(diff: f64, unit: Option<Unit>) -> String {
    let sign = if diff < 0.0 { '-' } else { '+' };
    format!("{sign}{}", format_value(diff.abs(), unit))
}

fn format_percent(diff: f64, previous: f64) -> String {
    if previous == 0.0 {
        return String::new();
    }
    // Widen precision as the change shrinks, so a real difference never
    // renders as `-0.0%`.
    let pct = diff / previous * 100.0;
    match pct.abs() {
        p if p >= 10.0 => format!("{pct:+.0}%"),
        p if p >= 1.0 => format!("{pct:+.1}%"),
        _ => format!("{pct:+.2}%"),
    }
}

fn format_millis(ms: f64) -> String {
    let secs = ms / 1000.0;
    if ms < 1000.0 {
        format!("{ms:.0}ms")
    } else if secs < 60.0 {
        format!("{secs:.1}s")
    } else {
        let mins = (secs / 60.0).trunc();
        format!("{mins:.0}m{:02.0}s", secs - mins * 60.0)
    }
}

fn format_bytes(bytes: f64) -> String {
    const SCALES: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes;
    let mut scale = 0;
    while value >= 1024.0 && scale < SCALES.len() - 1 {
        value /= 1024.0;
        scale += 1;
    }
    if scale == 0 {
        format!("{value:.0} B")
    } else {
        format!("{} {}", three_sig_figs(value), SCALES[scale])
    }
}

fn format_count(count: f64) -> String {
    let (divisor, suffix) = if count < 1_000.0 {
        return format_plain(count);
    } else if count < 1_000_000.0 {
        (1_000.0, "k")
    } else if count < 1_000_000_000.0 {
        (1_000_000.0, "M")
    } else {
        (1_000_000_000.0, "G")
    };
    format!("{} {}", three_sig_figs(count / divisor), suffix)
}

fn format_plain(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        three_sig_figs(value)
    }
}

/// Three significant figures, trailing zeros kept (`18.4`, `1.94`, `310`).
fn three_sig_figs(value: f64) -> String {
    let magnitude = value.abs();
    if magnitude >= 100.0 {
        format!("{value:.0}")
    } else if magnitude >= 10.0 {
        format!("{value:.1}")
    } else {
        format!("{value:.2}")
    }
}

fn format_dur(d: Duration) -> String {
    format_millis(d.as_secs_f64() * 1000.0)
}

/// `6 days ago` for a timestamp written by `utc_timestamp_rfc3339`. `None`
/// when either end of the comparison isn't a parseable timestamp.
fn relative_age(started_at: &str) -> Option<String> {
    let then = wezel_bench::run::unix_seconds(started_at).ok()?;
    let now = wezel_bench::run::unix_seconds(&wezel_bench::run::utc_timestamp_rfc3339()).ok()?;
    Some(humanize_age(now - then))
}

fn humanize_age(seconds: i64) -> String {
    /// `1 hour ago` / `6 hours ago`.
    fn plural(n: i64, unit: &str) -> String {
        if n == 1 {
            format!("{n} {unit} ago")
        } else {
            format!("{n} {unit}s ago")
        }
    }

    match seconds {
        s if s < 60 => "just now".to_string(),
        s if s < 3_600 => plural(s / 60, "minute"),
        s if s < 86_400 => plural(s / 3_600, "hour"),
        s if s < 86_400 * 14 => plural(s / 86_400, "day"),
        s => plural(s / (86_400 * 7), "week"),
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use wezel_bench::run::{ExperimentRunOutput, SummaryValue};
    use wezel_types::{ExperimentRunStep, ForagerPluginOutput, MetricDirection};

    use super::*;

    /// Outcome with `samples` values under `name`, all in `unit`.
    fn outcomes(
        name: &str,
        unit: Unit,
        direction: MetricDirection,
        values: &[f64],
    ) -> Vec<ForagerPluginOutput> {
        values
            .iter()
            .map(|v| ForagerPluginOutput {
                name: name.to_string(),
                value: serde_json::json!(v),
                direction,
                unit: Some(unit),
                tags: IndexMap::new(),
            })
            .collect()
    }

    fn summary_def(name: &str, step: &str, outcome: &str, samples: usize) -> SummaryDef {
        SummaryDef {
            name: name.to_string(),
            step: step.to_string(),
            measurement: outcome.to_string(),
            aggregation: samples.gt(&1).then_some(wezel_types::Aggregation::Median),
            filter: IndexMap::new(),
            bisect: true,
            samples,
        }
    }

    fn saved(commit: &str, started_at: &str, summaries: &[(&str, f64)]) -> SavedRun {
        SavedRun {
            schema_version: wezel_bench::run::SAVED_RUN_SCHEMA_VERSION,
            wezel_version: "0.0.0".into(),
            started_at: started_at.into(),
            duration_ms: 62_000,
            dirty: false,
            branch: Some("main".into()),
            output: ExperimentRunOutput {
                experiment: "workspace-build".into(),
                commit: commit.into(),
                steps: vec![
                    ExperimentRunStep {
                        index: Some(0),
                        step: "release-build".into(),
                        measurements: outcomes(
                            "time_ms",
                            Unit::Milliseconds,
                            MetricDirection::LowerIsBetter,
                            &[41_900.0, 42_300.0, 42_000.0, 42_100.0, 42_200.0],
                        ),
                        executions: Vec::new(),
                    },
                    ExperimentRunStep {
                        index: Some(1),
                        step: "artifacts".into(),
                        measurements: [
                            outcomes(
                                "wezel",
                                Unit::Bytes,
                                MetricDirection::LowerIsBetter,
                                &[19_293_798.0],
                            ),
                            outcomes(
                                "llvm-lines",
                                Unit::Count,
                                MetricDirection::LowerIsBetter,
                                &[1_940_000.0],
                            ),
                        ]
                        .concat(),
                        executions: Vec::new(),
                    },
                ],
                summaries: summaries
                    .iter()
                    .map(|(name, value)| {
                        (
                            (*name).to_string(),
                            SummaryValue {
                                value: *value,
                                bisect: true,
                            },
                        )
                    })
                    .collect(),
            },
        }
    }

    /// Full layout with a baseline. `render_run` reads the wall clock for the
    /// baseline's age, so the age phrase itself is checked separately.
    #[test]
    fn report_layout_matches_expected_columns() {
        let this_run = saved(
            "1a2b3c4d5e6f",
            "2026-08-15T12:00:00Z",
            &[
                ("build-time", 42_100.0),
                ("binary-size", 19_293_798.0),
                ("llvm-lines", 1_940_000.0),
            ],
        );
        let previous = saved(
            "8f21c4bdeadbe",
            "2026-08-09T12:00:00Z",
            &[
                ("build-time", 29_700.0),
                ("binary-size", 19_293_798.0),
                ("llvm-lines", 1_630_000.0),
            ],
        );
        let plan = vec![
            StepPlan {
                name: "release-build".into(),
                forager: "cargo".into(),
                samples: 5,
            },
            StepPlan {
                name: "artifacts".into(),
                forager: "filesize".into(),
                samples: 1,
            },
        ];
        let defs = vec![
            summary_def("build-time", "release-build", "time_ms", 5),
            summary_def("binary-size", "artifacts", "wezel", 1),
            summary_def("llvm-lines", "artifacts", "llvm-lines", 1),
        ];

        let measuring = IndexMap::from([
            ("release-build".to_string(), Duration::from_millis(211_000)),
            ("artifacts".to_string(), Duration::from_millis(3)),
        ]);
        let rendered = render_run(&this_run, &plan, &defs, &measuring, Some(&previous), false);
        let mut lines = rendered.lines();

        assert_eq!(
            lines.next().unwrap(),
            "workspace-build · 1a2b3c4 on main · 1m02s"
        );
        assert!(
            lines
                .next()
                .unwrap()
                .starts_with("against 8f21c4b · previous run,"),
            "{rendered}"
        );
        assert_eq!(lines.next().unwrap(), "");
        assert_eq!(lines.next().unwrap(), "step.cargo.release-build");
        assert_eq!(
            lines.next().unwrap(),
            "  build-time  5 samples  41.9s  42.3s  42.0s  42.1s  42.2s"
        );
        assert_eq!(lines.next().unwrap(), "");
        assert_eq!(
            lines.next().unwrap(),
            "SUMMARY      THIS RUN  PREVIOUS       Δ"
        );
        assert_eq!(lines.next().unwrap(), "─".repeat(45));
        assert_eq!(
            lines.next().unwrap(),
            "build-time      42.1s     29.7s  +12.4s  +42%"
        );
        assert_eq!(
            lines.next().unwrap(),
            "binary-size  18.4 MiB  18.4 MiB      ±0"
        );
        assert_eq!(
            lines.next().unwrap(),
            "llvm-lines     1.94 M    1.63 M  +310 k  +19%"
        );
        assert_eq!(lines.next(), None);
    }

    #[test]
    fn first_run_says_there_is_nothing_to_compare() {
        let this_run = saved(
            "1a2b3c4d5e6f",
            "2026-08-15T12:00:00Z",
            &[("build-time", 42_100.0)],
        );
        let plan = vec![StepPlan {
            name: "release-build".into(),
            forager: "cargo".into(),
            samples: 5,
        }];
        let defs = vec![summary_def("build-time", "release-build", "time_ms", 5)];

        let rendered = render_run(&this_run, &plan, &defs, &IndexMap::new(), None, false);
        assert!(
            rendered.contains("no previous run to compare against"),
            "{rendered}"
        );
        // No baseline: the previous and delta cells stay empty rather than
        // claiming a change.
        assert!(
            rendered.contains("build-time     42.1s         —"),
            "{rendered}"
        );
    }

    #[test]
    fn milliseconds_scale_to_seconds_and_minutes() {
        assert_eq!(format_value(421.0, Some(Unit::Milliseconds)), "421ms");
        assert_eq!(format_value(42_100.0, Some(Unit::Milliseconds)), "42.1s");
        assert_eq!(format_value(62_000.0, Some(Unit::Milliseconds)), "1m02s");
    }

    #[test]
    fn bytes_use_binary_scales() {
        assert_eq!(format_value(512.0, Some(Unit::Bytes)), "512 B");
        assert_eq!(
            format_value(18.4 * 1024.0 * 1024.0, Some(Unit::Bytes)),
            "18.4 MiB"
        );
    }

    #[test]
    fn counts_use_si_suffixes() {
        assert_eq!(format_value(842.0, Some(Unit::Count)), "842");
        assert_eq!(format_value(310_000.0, Some(Unit::Count)), "310 k");
        assert_eq!(format_value(1_940_000.0, Some(Unit::Count)), "1.94 M");
    }

    #[test]
    fn unitless_values_print_raw() {
        assert_eq!(format_value(42_100.0, None), "42100");
        assert_eq!(format_value(1.5, None), "1.50");
    }

    #[test]
    fn deltas_carry_an_explicit_sign() {
        assert_eq!(format_signed(12_400.0, Some(Unit::Milliseconds)), "+12.4s");
        assert_eq!(format_signed(-310_000.0, Some(Unit::Count)), "-310 k");
    }

    #[test]
    fn percent_precision_widens_below_ten() {
        assert_eq!(format_percent(12_400.0, 29_700.0), "+42%");
        assert_eq!(format_percent(-100.0, 29_700.0), "-0.34%");
        assert_eq!(format_percent(-2_240.0, 10_903_088.0), "-0.02%");
        assert_eq!(format_percent(400.0, 29_700.0), "+1.3%");
        assert_eq!(format_percent(5.0, 0.0), "");
    }

    #[test]
    fn ages_read_as_english() {
        assert_eq!(humanize_age(30), "just now");
        assert_eq!(humanize_age(3_600), "1 hour ago");
        assert_eq!(humanize_age(86_400 * 6), "6 days ago");
        assert_eq!(humanize_age(86_400 * 21), "3 weeks ago");
    }
}
