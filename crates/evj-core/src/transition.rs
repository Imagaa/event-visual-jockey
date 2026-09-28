//! Transitions between clips: presets (shader + duration + easing) and their running progress.
use crate::effect::ParamValue;
use crate::model::{Clip, Layer};
use serde::{Deserialize, Serialize};

/// Shader name meaning "no shader, switch instantly".
pub const CUT: &str = "Cut";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TransitionTime {
    Seconds(f64),
    Beats(f64),
}

impl TransitionTime {
    pub fn seconds(self, bpm: f64) -> f64 {
        match self {
            TransitionTime::Seconds(s) => s.max(0.0),
            TransitionTime::Beats(b) => b.max(0.0) * 60.0 / bpm.max(1.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Easing {
    #[default]
    Linear,
    In,
    Out,
    InOut,
}

impl Easing {
    pub fn apply(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            Easing::Linear => t,
            Easing::In => t * t,
            Easing::Out => 1.0 - (1.0 - t) * (1.0 - t),
            Easing::InOut => t * t * (3.0 - 2.0 * t),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TransitionPreset {
    pub name: String,
    /// Transition shader in the effect library, or [`CUT`].
    pub shader: String,
    pub time: TransitionTime,
    pub easing: Easing,
    pub params: Vec<ParamValue>,
    pub favorite: bool,
}

impl Default for TransitionPreset {
    fn default() -> Self {
        TransitionPreset::cut()
    }
}

impl TransitionPreset {
    fn new(name: &str, shader: &str, time: TransitionTime, favorite: bool) -> TransitionPreset {
        TransitionPreset { name: name.into(), shader: shader.into(), time, easing: Easing::InOut, params: Vec::new(), favorite }
    }

    pub fn cut() -> TransitionPreset {
        TransitionPreset::new(CUT, CUT, TransitionTime::Seconds(0.0), true)
    }

    pub fn crossfade() -> TransitionPreset {
        TransitionPreset::new("Crossfade", "Crossfade", TransitionTime::Seconds(1.0), true)
    }

    pub fn is_cut(&self) -> bool {
        self.shader == CUT
    }

    /// Presets a new project starts with.
    pub fn defaults() -> Vec<TransitionPreset> {
        use TransitionTime::*;
        vec![
            TransitionPreset::cut(),
            TransitionPreset::crossfade(),
            TransitionPreset::new("Dip to Black", "Dip to Black", Seconds(1.0), true),
            TransitionPreset::new("Dip to White", "Flash", Seconds(0.8), false),
            TransitionPreset::new("Wipe", "Wipe", Beats(0.5), true),
            TransitionPreset::new("Slide", "Slide", Beats(1.0), false),
            TransitionPreset::new("Zoom", "Zoom", Seconds(1.0), false),
            TransitionPreset::new("Dissolve", "Dissolve", Seconds(1.5), false),
            TransitionPreset::new("Luma Fade", "Luma Fade", Seconds(1.5), false),
            TransitionPreset::new("Blur Fade", "Blur Fade", Seconds(1.0), false),
            TransitionPreset::new("Pixelate", "Pixelate", Beats(1.0), false),
            TransitionPreset::new("Radial", "Radial", Seconds(1.0), false),
            TransitionPreset::new("Additive", "Additive", Seconds(1.0), false),
        ]
    }
}

/// Which preset a trigger uses: clip override, then the live "next" choice, then the layer default. None = cut.
pub fn resolve_preset<'a>(presets: &'a [TransitionPreset], clip: &Clip, next: Option<&str>, layer: &Layer) -> Option<&'a TransitionPreset> {
    let name = clip.transition.as_deref().or(next).or(layer.transition.as_deref())?;
    presets.iter().find(|p| p.name == name).filter(|p| !p.is_cut())
}

/// A transition in progress.
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionRun {
    pub preset: TransitionPreset,
    elapsed: f64,
    duration: f64,
}

impl TransitionRun {
    pub fn new(preset: &TransitionPreset, bpm: f64) -> TransitionRun {
        let duration = if preset.is_cut() { 0.0 } else { preset.time.seconds(bpm) };
        TransitionRun { preset: preset.clone(), elapsed: 0.0, duration }
    }

    pub fn advance(&mut self, dt: f64) {
        self.elapsed += dt;
    }

    /// Eased 0..1.
    pub fn progress(&self) -> f64 {
        if self.duration <= 0.0 {
            return 1.0;
        }
        self.preset.easing.apply(self.elapsed / self.duration)
    }

    /// Seconds until the transition ends.
    pub fn remaining(&self) -> f64 {
        (self.duration - self.elapsed).max(0.0)
    }

    pub fn done(&self) -> bool {
        self.elapsed >= self.duration
    }
}
