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
    step_start: Option<Instant>,
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
    fn run_started(&self, experiment: &str, commit: &str, steps: &[StepPlan]) {
        let short = &commit[..7.min(commit.len())];
        let _ = self
            .multi
            .println(format!("Experiment: {experiment}  @  {short}"));

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
        state.timings.insert(
            step.to_string(),
            Timing {
                step_start: Some(Instant::now()),
                ..Default::default()
            },
        );
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
        let timing = state.timings.get(step);
        let work = timing.map(|t| t.accumulated).unwrap_or_default();
        let total = timing
            .and_then(|t| t.step_start.map(|s| s.elapsed()))
            .unwrap_or(work);
        let setup = total.saturating_sub(work);
        if let Some(pb) = state.bars.get(step) {
            pb.disable_steady_tick();
            pb.set_style(done_style());
            pb.set_message(format!(
                "{} setup + {} work",
                format_dur(setup),
                format_dur(work)
            ));
            pb.finish();
        }
    }

    fn run_finished(&self) {
        let _ = self.multi.println("");
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
    let ms = d.subsec_millis();
    if secs >= 60 {
        let m = secs / 60;
        let s = secs % 60;
        format!("{m}m{s:02}s")
    } else if secs >= 10 {
        format!("{secs}s")
    } else {
        format!("{secs}.{ms:03}s")
    }
}
