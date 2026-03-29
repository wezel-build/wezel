use std::sync::{Arc, OnceLock};

/// Decides whether a new measurement constitutes a regression and classifies
/// values during bisection.
///
/// Implementations can range from a simple percentage threshold to statistical
/// approaches like 3-sigma detection.
pub trait RegressionDetector: Send + Sync {
    /// Returns `true` if `new_value` is a regression given recent history.
    ///
    /// `history` contains the most recent values for this measurement
    /// (oldest first). May be empty if no prior data exists.
    fn is_regression(&self, history: &[f64], new_value: f64) -> bool;

    /// During bisection, classify a value as "good" (closer to the good end)
    /// or "bad" (closer to the bad end).
    fn is_good(&self, good_value: f64, bad_value: f64, value: f64) -> bool;

    /// How many historical values this detector needs to make a decision.
    /// `detect_regressions` will query this many prior measurements.
    fn history_len(&self) -> usize;
}

/// Simple percentage-threshold detector: flags a regression when the value
/// increases by more than `threshold` (e.g. 0.05 = 5%).
pub struct ThresholdDetector {
    pub threshold: f64,
}

impl Default for ThresholdDetector {
    fn default() -> Self {
        Self { threshold: 0.05 }
    }
}

impl RegressionDetector for ThresholdDetector {
    fn is_regression(&self, history: &[f64], new_value: f64) -> bool {
        let Some(&old_value) = history.last() else {
            return false;
        };
        if old_value == 0.0 {
            return false;
        }
        (new_value - old_value) / old_value.abs() > self.threshold
    }

    fn is_good(&self, good_value: f64, _bad_value: f64, value: f64) -> bool {
        value <= good_value * (1.0 + self.threshold)
    }

    fn history_len(&self) -> usize {
        1
    }
}

static DETECTOR: OnceLock<Arc<dyn RegressionDetector>> = OnceLock::new();

pub fn set_detector(detector: Arc<dyn RegressionDetector>) {
    if DETECTOR.set(detector).is_err() {
        panic!("detector already set");
    }
}

pub fn detector() -> &'static Arc<dyn RegressionDetector> {
    DETECTOR
        .get()
        .expect("regression detector not initialized")
}
