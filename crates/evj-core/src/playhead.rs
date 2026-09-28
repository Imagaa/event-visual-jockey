//! Clip playhead: where in the clip (seconds) we are, per play mode, speed and in/out range.
use crate::model::{Clip, PlayMode};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Playhead {
    pub pos: f64,
    /// Ping-pong direction multiplier (+1 / -1).
    pub dir: f64,
    pub finished: bool,
}

/// The clip's in / out points in seconds, ordered.
pub fn in_out(clip: &Clip, duration: f64) -> (f64, f64) {
    let a = clip.in_point.clamp(0.0, 1.0) * duration;
    let b = clip.out_point.clamp(0.0, 1.0) * duration;
    (a.min(b), a.max(b))
}

/// Speed after BPM sync: the in/out range is stretched to `bpm_beats` beats.
pub fn effective_speed(clip: &Clip, duration: f64, bpm: f64) -> f64 {
    match clip.bpm_beats {
        Some(beats) if beats > 0.0 && bpm > 0.0 => {
            let (a, b) = in_out(clip, duration);
            let sign = if clip.speed < 0.0 { -1.0 } else { 1.0 };
            sign * (b - a) / (beats * 60.0 / bpm)
        }
        _ => clip.speed,
    }
}

impl Playhead {
    pub fn start(clip: &Clip, duration: f64) -> Playhead {
        let (a, b) = in_out(clip, duration);
        Playhead { pos: if clip.speed < 0.0 { b } else { a }, dir: 1.0, finished: false }
    }

    pub fn advance(&mut self, dt: f64, clip: &Clip, duration: f64, bpm: f64) {
        let (a, b) = in_out(clip, duration);
        let len = b - a;
        if len <= 0.0 || self.finished {
            self.pos = self.pos.clamp(a, b.max(a));
            return;
        }
        let v = effective_speed(clip, duration, bpm) * self.dir;
        let pos = self.pos + v * dt;
        match clip.mode {
            PlayMode::Loop => self.pos = a + (pos - a).rem_euclid(len),
            PlayMode::Once => {
                self.pos = pos.clamp(a, b);
                self.finished = (v > 0.0 && pos >= b) || (v < 0.0 && pos <= a);
            }
            PlayMode::PingPong => {
                // Fold the position into [a, b] over a 2·len period; odd half-periods run backwards.
                let t = (pos - a).rem_euclid(2.0 * len);
                let bounces = ((pos - a) / len).floor() as i64;
                self.pos = if t <= len { a + t } else { b - (t - len) };
                if bounces.rem_euclid(2) == 1 {
                    self.dir = -self.dir;
                }
            }
        }
    }
}

/// Seconds left until the clip reaches its out point (forward) or in point (reverse) at
/// `velocity` clip-seconds per second; infinite when not moving.
pub fn remaining(pos: f64, a: f64, b: f64, velocity: f64) -> f64 {
    if velocity == 0.0 {
        return f64::INFINITY;
    }
    let left = if velocity > 0.0 { b - pos } else { pos - a };
    (left / velocity.abs()).max(0.0)
}

/// Frame to show at `pos` seconds, clamped to the clip.
pub fn frame_index(pos: f64, fps: f64, frame_count: u64) -> u64 {
    let i = (pos * fps + 1e-6).floor().max(0.0) as u64;
    i.min(frame_count.saturating_sub(1))
}
