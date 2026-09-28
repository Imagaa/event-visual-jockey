//! Beat-synced low-frequency oscillators for effect parameters.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Wave {
    #[default]
    Sine,
    Saw,
    Square,
    Triangle,
    /// Sample & hold: a new random value every period.
    Random,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Lfo {
    pub wave: Wave,
    /// Period length in beats.
    pub beats: f64,
    /// Output range, normalised to the parameter's range (0..1).
    pub min: f64,
    pub max: f64,
}

impl Default for Lfo {
    fn default() -> Self {
        Lfo { wave: Wave::Sine, beats: 4.0, min: 0.0, max: 1.0 }
    }
}

fn hash01(n: i64) -> f64 {
    // splitmix64
    let mut z = (n as u64).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    (z >> 11) as f64 / (1u64 << 53) as f64
}

impl Lfo {
    /// Value at `beat`, within `min..max`.
    pub fn value(&self, beat: f64) -> f64 {
        let period = self.beats.max(1e-3);
        let t = (beat / period).rem_euclid(1.0);
        let u = match self.wave {
            Wave::Sine => 0.5 + 0.5 * (t * std::f64::consts::TAU).sin(),
            Wave::Saw => t,
            Wave::Square => {
                if t < 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            Wave::Triangle => 1.0 - (2.0 * t - 1.0).abs(),
            Wave::Random => hash01((beat / period).floor() as i64),
        };
        self.min + (self.max - self.min) * u
    }
}
