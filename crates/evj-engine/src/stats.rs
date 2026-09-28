//! Frame-time statistics over a sliding window (memory stays flat during a 4-hour show).
use std::collections::VecDeque;

/// Frames kept for the rolling numbers (10 s at 60 Hz).
pub const WINDOW: usize = 600;

/// A frame counts as dropped when it took longer than 1.5 refresh periods.
#[derive(Default)]
pub struct FrameStats {
    recent: VecDeque<f32>,
    pub dropped: u64,
    pub total: u64,
}

impl FrameStats {
    pub fn record(&mut self, dt_ms: f32, refresh_ms: f32) {
        if self.recent.len() == WINDOW {
            self.recent.pop_front();
        }
        self.recent.push_back(dt_ms);
        self.total += 1;
        if dt_ms > refresh_ms * 1.5 {
            self.dropped += 1;
        }
    }

    pub fn recent_percentile(&self, p: f32) -> f32 {
        if self.recent.is_empty() {
            return 0.0;
        }
        let mut v: Vec<f32> = self.recent.iter().copied().collect();
        v.sort_by(f32::total_cmp);
        v[(((p / 100.0) * (v.len() - 1) as f32).round() as usize).min(v.len() - 1)]
    }

    pub fn recent_fps(&self) -> f32 {
        if self.recent.is_empty() {
            return 0.0;
        }
        1000.0 * self.recent.len() as f32 / self.recent.iter().sum::<f32>()
    }

    /// The last `n` frame times (ms), oldest first.
    pub fn tail(&self, n: usize) -> Vec<f32> {
        self.recent.iter().skip(self.recent.len().saturating_sub(n)).copied().collect()
    }

    /// Clears the counters (e.g. after warm-up), keeps nothing.
    pub fn reset(&mut self) {
        *self = FrameStats::default();
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameStats, WINDOW};

    #[test]
    fn percentiles_drops_and_fps() {
        let mut s = FrameStats::default();
        for _ in 0..98 {
            s.record(16.6, 16.67);
        }
        s.record(40.0, 16.67);
        s.record(17.0, 16.67);
        assert_eq!(s.dropped, 1);
        assert_eq!(s.total, 100);
        assert!((s.recent_percentile(50.0) - 16.6).abs() < 0.01);
        assert!((s.recent_percentile(100.0) - 40.0).abs() < 0.01);
        assert!((s.recent_fps() - 1000.0 * 100.0 / (98.0 * 16.6 + 57.0)).abs() < 0.01);
    }

    #[test]
    fn window_is_bounded_but_counters_are_not() {
        let mut s = FrameStats::default();
        for _ in 0..WINDOW * 3 {
            s.record(50.0, 16.67);
        }
        for _ in 0..WINDOW {
            s.record(10.0, 16.67);
        }
        assert_eq!(s.total, (WINDOW * 4) as u64);
        assert_eq!(s.dropped, (WINDOW * 3) as u64);
        assert!((s.recent_percentile(100.0) - 10.0).abs() < 0.01, "old frames left the window");
    }

    #[test]
    fn empty_is_zero() {
        let s = FrameStats::default();
        assert_eq!((s.recent_percentile(99.0), s.recent_fps()), (0.0, 0.0));
    }
}
