//! Project data model — what gets saved in a `.vjproj` and edited by the UI.
use crate::effect::EffectRef;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PlayMode {
    #[default]
    Loop,
    PingPong,
    Once,
}

/// How a clip is scaled into the composition. Empty areas stay transparent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FitMode {
    #[default]
    Fit,
    Fill,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BlendMode {
    #[default]
    Alpha,
    Add,
    Screen,
    Multiply,
    Overlay,
    Difference,
    Lighten,
    Darken,
    Subtract,
    LumaKey,
}

impl BlendMode {
    pub const ALL: [BlendMode; 10] = [
        BlendMode::Alpha,
        BlendMode::Add,
        BlendMode::Screen,
        BlendMode::Multiply,
        BlendMode::Overlay,
        BlendMode::Difference,
        BlendMode::Lighten,
        BlendMode::Darken,
        BlendMode::Subtract,
        BlendMode::LumaKey,
    ];

    /// Shader index — order of `ALL`.
    pub fn index(self) -> u32 {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0) as u32
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clip {
    pub path: PathBuf,
    pub name: String,
    pub mode: PlayMode,
    /// Negative plays backwards (random-access sources only).
    pub speed: f64,
    /// Normalised 0..1 range within the clip.
    pub in_point: f64,
    pub out_point: f64,
    /// When set, the in/out range is stretched to this many beats.
    pub bpm_beats: Option<f64>,
    pub fit: FitMode,
    pub effects: Vec<EffectRef>,
    /// Transition preset used when this clip is triggered (overrides the layer's).
    pub transition: Option<String>,
    /// Play the file's sound (video with audio, MP3/WAV clips).
    pub audio: bool,
    /// Extra audio file played with the picture.
    pub attached: Option<AttachedAudio>,
    /// A still image ends after this many seconds (sequences); None = stays up.
    pub still_secs: Option<f64>,
    /// Has been on PROGRAM (a mark on the slot).
    pub aired: bool,
}

/// Extra sound played with a picture clip. Replace mutes the clip's own sound.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AttachedAudio {
    pub path: PathBuf,
    /// true = Mix with the clip's own sound, false = Replace it.
    pub mix: bool,
    pub volume: f32,
}

impl Default for AttachedAudio {
    fn default() -> Self {
        AttachedAudio { path: PathBuf::new(), mix: false, volume: 1.0 }
    }
}

/// Slots of one layer that play one after another (column order).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sequence {
    pub layer: usize,
    pub cols: Vec<usize>,
    /// After the last clip, start again with the first.
    pub loop_all: bool,
    /// How long a still image stays up (seconds).
    pub still_secs: f64,
}

impl Default for Sequence {
    fn default() -> Self {
        Sequence { layer: 0, cols: Vec::new(), loop_all: false, still_secs: 5.0 }
    }
}

impl Default for Clip {
    fn default() -> Self {
        Clip::new(PathBuf::new())
    }
}

impl Clip {
    pub fn new(path: PathBuf) -> Clip {
        let name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        Clip { path, name, mode: PlayMode::Loop, speed: 1.0, in_point: 0.0, out_point: 1.0, bpm_beats: None, fit: FitMode::Fit, effects: Vec::new(), transition: None, audio: true, attached: None, still_secs: None, aired: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layer {
    pub name: String,
    pub opacity: f32,
    pub blend: BlendMode,
    pub bypass: bool,
    pub solo: bool,
    pub effects: Vec<EffectRef>,
    /// Default transition preset for clips triggered on this layer.
    pub transition: Option<String>,
    /// Audio level of the layer's clips (0..1).
    pub volume: f32,
    pub mute: bool,
}

impl Default for Layer {
    fn default() -> Self {
        Layer { name: String::new(), opacity: 1.0, blend: BlendMode::Alpha, bypass: false, solo: false, effects: Vec::new(), transition: None, volume: 1.0, mute: false }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Deck {
    pub name: String,
    /// `slots[layer][column]`.
    pub slots: Vec<Vec<Option<Clip>>>,
    pub sequences: Vec<Sequence>,
    /// Column (scene) names; empty = "Scene N".
    pub scene_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Composition {
    pub width: u32,
    pub height: u32,
    pub bpm: f64,
    pub layers: Vec<Layer>,
    pub effects: Vec<EffectRef>,
}

impl Default for Composition {
    fn default() -> Self {
        Composition { width: 1920, height: 1080, bpm: 120.0, layers: Vec::new(), effects: Vec::new() }
    }
}

/// One presentation in the event's rundown (imported PPTX / PDF).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct MateriItem {
    pub title: String,
    /// The deck's `deck.json`.
    pub deck: PathBuf,
    /// Ticked off in the checklist once presented.
    pub done: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Project {
    pub composition: Composition,
    pub decks: Vec<Deck>,
    pub active_deck: usize,
    pub keymap: crate::keymap::KeyMap,
    pub transitions: Vec<crate::transition::TransitionPreset>,
    pub outputs: Vec<crate::output::OutputConfig>,
    /// Presentation rundown (checklist).
    pub materi: Vec<MateriItem>,
    /// Layer that shows presentations; None = the top layer.
    pub presentation_layer: Option<usize>,
    /// Slot played full-frame by PANIC: (deck, layer, column). None = deck 1, layer 1, column 1.
    pub panic_media: Option<(usize, usize, usize)>,
}

pub const DEFAULT_LAYERS: usize = 4;
pub const DEFAULT_COLUMNS: usize = 8;

impl Project {
    pub fn presentation_layer(&self) -> usize {
        let top = self.composition.layers.len().saturating_sub(1);
        self.presentation_layer.map_or(top, |l| l.min(top))
    }

    /// Old shows: the materi list becomes presentation clips in the grid (first empty columns
    /// of the presentation layer, on the active deck). true when something moved.
    pub fn migrate_materi(&mut self) -> bool {
        if self.materi.is_empty() || self.decks.is_empty() {
            return false;
        }
        let layer = self.presentation_layer();
        let items = std::mem::take(&mut self.materi);
        let d = self.active_deck.min(self.decks.len() - 1);
        for m in items {
            let deck = &mut self.decks[d];
            let col = deck.slots.get(layer).and_then(|r| r.iter().position(Option::is_none)).unwrap_or_else(|| deck.slots.first().map_or(0, Vec::len));
            deck.place(layer, col, std::slice::from_ref(&m.deck));
            if let Some(c) = deck.clip_mut(layer, col) {
                c.name = m.title;
                c.aired = m.done;
            }
        }
        let cols = self.columns();
        let n = self.composition.layers.len();
        for deck in &mut self.decks {
            deck.ensure_size(n, cols);
        }
        true
    }

    pub fn new_default() -> Project {
        let layers = (0..DEFAULT_LAYERS).map(|i| Layer { name: format!("Layer {}", i + 1), ..Layer::default() }).collect();
        let deck = Deck { name: "Deck 1".into(), slots: vec![vec![None; DEFAULT_COLUMNS]; DEFAULT_LAYERS], ..Default::default() };
        Project { composition: Composition { layers, ..Composition::default() }, decks: vec![deck], active_deck: 0, keymap: Default::default(), transitions: crate::transition::TransitionPreset::defaults(), outputs: vec![Default::default()], materi: Vec::new(), presentation_layer: None, panic_media: None }
    }
}
