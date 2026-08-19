use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use wezel_bench::run::{RunReporter, StepPlan};

/// Per-step progress bar layout via indicatif. Bars render to stderr and are
/// auto-disabled when stderr isn't a TTY.
///
/// Displayed time is forager-only — snapshot capture and inter-sample
/// restores are excluded by accumulating duration between paired
/// `sample_started`/`sample_done` events rather than using indicatif's
/// wall-clock `{elapsed}`.
pub struct IndicatifReporter {
    multi: MultiProgress,
    state: Mutex<State>,
}

struct State {
    bars: HashMap<String, ProgressBar>,
    plan: HashMap<String, usize>,
    name_width: usize,
    timings: HashMap<String, Timing>,
}

#[derive(Default)]
struct Timing {
    /// Forager-only elapsed (between paired sample_started/sample_done).
    accumulated: Duration,
    sample_start: Option<Instant>,
}

impl IndicatifReporter {
    pub fn new() -> Self {
        Self {
            multi: MultiProgress::new(),
            state: Mutex::new(State {
                bars: HashMap::new(),
                plan: HashMap::new(),
                name_width: 0,
                timings: HashMap::new(),
            }),
        }
    }
}

impl Default for IndicatifReporter {
    fn default() -> Self {
        Self::new()
    }
}

impl RunReporter for IndicatifReporter {
    /// No banner here — the final report prints the run's identity. Bars are
    /// live UI only, cleared in `run_finished`.
    fn run_started(&self, _experiment: &str, _commit: &str, steps: &[StepPlan]) {
        let mut state = self.state.lock().unwrap();
        state.name_width = steps.iter().map(|s| s.name.len()).max().unwrap_or(0);
        state.plan = steps.iter().map(|s| (s.name.clone(), s.samples)).collect();
    }

    fn step_started(&self, step: &str) {
        let mut state = self.state.lock().unwrap();
        let samples = state.plan.get(step).copied().unwrap_or(1) as u64;
        let prefix = format!("{:<width$}", step, width = state.name_width);
        let pb = self.multi.add(ProgressBar::new(samples));
        pb.set_style(running_style());
        pb.set_prefix(prefix);
        pb.set_message(if samples > 1 { "preparing…" } else { "" }.to_string());
        pb.enable_steady_tick(Duration::from_millis(100));
        state.bars.insert(step.to_string(), pb);
        state.timings.insert(step.to_string(), Timing::default());
    }

    fn sample_started(&self, step: &str, _iter: usize, _samples: usize) {
        let mut state = self.state.lock().unwrap();
        if let Some(t) = state.timings.get_mut(step) {
            t.sample_start = Some(Instant::now());
        }
        let acc = state
            .timings
            .get(step)
            .map(|t| t.accumulated)
            .unwrap_or_default();
        if let Some(pb) = state.bars.get(step) {
            pb.set_message(format_dur(acc));
        }
    }

    fn sample_done(&self, step: &str, _iter: usize, _samples: usize) {
        let mut state = self.state.lock().unwrap();
        if let Some(t) = state.timings.get_mut(step)
            && let Some(start) = t.sample_start.take()
        {
            t.accumulated += start.elapsed();
        }
        let acc = state
            .timings
            .get(step)
            .map(|t| t.accumulated)
            .unwrap_or_default();
        if let Some(pb) = state.bars.get(step) {
            pb.inc(1);
            pb.set_message(format_dur(acc));
        }
    }

    fn step_finished(&self, step: &str) {
        let state = self.state.lock().unwrap();
        let work = state
            .timings
            .get(step)
            .map(|t| t.accumulated)
            .unwrap_or_default();
        if let Some(pb) = state.bars.get(step) {
            pb.disable_steady_tick();
            pb.set_style(done_style());
            pb.set_message(format_dur(work));
            pb.finish();
        }
    }

    /// Clear every bar so the run's only lasting output is the report. Per-step
    /// timing survives in `CompletedRun::measuring_by_step`.
    fn run_finished(&self) {
        let state = self.state.lock().unwrap();
        for pb in state.bars.values() {
            pb.finish_and_clear();
        }
        let _ = self.multi.clear();
    }
}

fn running_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "  {spinner:.cyan} {prefix}  [{bar:24.cyan/blue}] {pos}/{len}  {msg}",
    )
    .unwrap()
    .progress_chars("=> ")
}

fn done_style() -> ProgressStyle {
    ProgressStyle::with_template("  {prefix:.green}  [{bar:24.green/green}] {pos}/{len}  {msg}")
        .unwrap()
        .progress_chars("== ")
}

fn format_dur(d: Duration) -> String {
    let secs = d.as_secs();
    if secs >= 60 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else if d.as_millis() >= 1000 {
        format!("{:.1}s", d.as_secs_f64())
    } else {
        format!("{}ms", d.as_millis())
    }
}
