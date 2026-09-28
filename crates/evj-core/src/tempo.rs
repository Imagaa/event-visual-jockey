//! Global tempo: BPM, running beat count, tap tempo.

/// Taps further apart than this start a new tap sequence.
const TAP_RESET_SECS: f64 = 2.0;
/// Intervals averaged for tap tempo.
const TAP_WINDOW: usize = 4;

#[derive(Debug, Clone)]
pub struct Tempo {
    pub bpm: f64,
    /// Beats since start (fraction = phase within the beat).
    pub beat: f64,
    taps: Vec<f64>,
}

impl Tempo {
    pub fn new(bpm: f64) -> Tempo {
        Tempo { bpm: bpm.clamp(1.0, 999.0), beat: 0.0, taps: Vec::new() }
    }

    pub fn set_bpm(&mut self, bpm: f64) {
        self.bpm = bpm.clamp(1.0, 999.0);
    }

    pub fn advance(&mut self, dt: f64) {
        self.beat += dt * self.bpm / 60.0;
    }

    /// 0..1 within the current beat.
    pub fn phase(&self) -> f64 {
        self.beat.rem_euclid(1.0)
    }

    /// `now` in seconds on any monotonic clock.
    pub fn tap(&mut self, now: f64) {
        if self.taps.last().is_some_and(|&last| now - last > TAP_RESET_SECS || now <= last) {
            self.taps.clear();
        }
        self.taps.push(now);
        if self.taps.len() > TAP_WINDOW + 1 {
            self.taps.remove(0);
        }
        if self.taps.len() >= 2 {
            let span = self.taps[self.taps.len() - 1] - self.taps[0];
            self.set_bpm(60.0 * (self.taps.len() - 1) as f64 / span);
            self.beat = self.beat.round(); // the tap is on the beat
        }
    }

    /// "Now is beat one."
    pub fn resync(&mut self) {
        self.beat = 0.0;
    }

    /// Shifts the phase by a fraction of a beat (catch up with / hold back from the music).
    pub fn nudge(&mut self, beats: f64) {
        self.beat += beats;
    }
}
