use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use wezel_bench::run::{RunReporter, StepPlan};

/// Per-step progress bar layout via indicatif. Bars render to stderr and are
/// auto-disabled when stderr isn't a TTY.
pub struct IndicatifReporter {
    multi: MultiProgress,
    state: Mutex<State>,
}

struct State {
    bars: HashMap<String, ProgressBar>,
    plan: HashMap<String, usize>,
    name_width: usize,
}

impl IndicatifReporter {
    pub fn new() -> Self {
        Self {
            multi: MultiProgress::new(),
            state: Mutex::new(State {
                bars: HashMap::new(),
                plan: HashMap::new(),
                name_width: 0,
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
        pb.enable_steady_tick(Duration::from_millis(100));
        state.bars.insert(step.to_string(), pb);
    }

    fn sample_done(&self, step: &str, _iter: usize, _samples: usize) {
        let state = self.state.lock().unwrap();
        if let Some(pb) = state.bars.get(step) {
            pb.inc(1);
        }
    }

    fn step_finished(&self, step: &str) {
        let state = self.state.lock().unwrap();
        if let Some(pb) = state.bars.get(step) {
            pb.disable_steady_tick();
            pb.set_style(done_style());
            pb.finish();
        }
    }

    fn run_finished(&self) {
        let _ = self.multi.println("");
    }
}

fn running_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "  {spinner:.cyan} {prefix}  [{bar:24.cyan/blue}] {pos}/{len}  {elapsed}",
    )
    .unwrap()
    .progress_chars("=> ")
}

fn done_style() -> ProgressStyle {
    ProgressStyle::with_template(
        "  {prefix:.green}  [{bar:24.green/green}] {pos}/{len}  done in {elapsed}",
    )
    .unwrap()
    .progress_chars("== ")
}
