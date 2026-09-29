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
    /// How long an image plays (seconds); None = [`DEFAULT_IMAGE_SECS`].
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

/// Slots of one layer that play one after another (column order). Old shows only:
/// [`Project::migrate`] turns them into scene chains.
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

/// When a layer-chain step starts (the first step starts the chain).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum StepStart {
    /// N seconds after the previous step started.
    AfterSecs(f64),
    /// When the previous step's clip ends.
    #[default]
    AfterPrevious,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum StepMode {
    /// The previous step's layer is cleared.
    #[default]
    Replace,
    /// The previous step's layer keeps playing underneath / above.
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ChainStep {
    pub layer: usize,
    pub start: StepStart,
    pub mode: StepMode,
}

/// Slots of several layers in one scene that start one after another (layer order).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct LayerChain {
    pub col: usize,
    pub steps: Vec<ChainStep>,
    /// After the last step: clear the chain's layers and start again (else stop).
    pub looping: bool,
}

/// Scenes that play one after another (column order).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SceneChain {
    pub cols: Vec<usize>,
    pub looping: bool,
}

pub const DEFAULT_IMAGE_SECS: f64 = 5.0;

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

    /// How long this clip plays when it is an image.
    pub fn image_secs(&self) -> f64 {
        self.still_secs.unwrap_or(DEFAULT_IMAGE_SECS)
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
    pub layer_chains: Vec<LayerChain>,
    pub scene_chains: Vec<SceneChain>,
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
    /// 0 = old shows (the last layer drawn on top); 1 = Layer 1 on top. See [`Project::migrate`].
    pub layer_order: u8,
}

pub const DEFAULT_LAYERS: usize = 4;
pub const DEFAULT_COLUMNS: usize = 8;

impl Project {
    /// Defaults to Layer 1 (the top layer).
    pub fn presentation_layer(&self) -> usize {
        let last = self.composition.layers.len().saturating_sub(1);
        self.presentation_layer.map_or(0, |l| l.min(last))
    }

    /// Brings an old show up to date: sequences become scene chains, and the layers are
    /// reversed so Layer 1 is the top one (the show looks the same). true when something changed.
    pub fn migrate(&mut self) -> bool {
        let mut changed = false;
        for d in &mut self.decks {
            d.prune_chains(); // a hand-edited / damaged file must not break the grid
            for s in std::mem::take(&mut d.sequences) {
                changed = true;
                // The sequence's still time becomes its images' own duration (ignored for video).
                for &c in &s.cols {
                    if let Some(clip) = d.clip_mut(s.layer, c) {
                        clip.still_secs.get_or_insert(s.still_secs);
                    }
                }
                if d.make_scene_chain(&s.cols) {
                    d.scene_chains.last_mut().unwrap().looping = s.loop_all;
                }
            }
        }
        if self.layer_order == 0 {
            let n = self.composition.layers.len();
            let cols = self.columns();
            self.remap_layers(|l| n - 1 - l);
            self.composition.layers.reverse();
            for d in &mut self.decks {
                d.ensure_size(n, cols);
                d.slots.reverse();
            }
            for (i, layer) in self.composition.layers.iter_mut().enumerate() {
                if layer.name == format!("Layer {}", n - i) {
                    layer.name = format!("Layer {}", i + 1);
                }
            }
            self.layer_order = 1;
            changed = true;
        }
        changed
    }

    /// Points everything that names a layer (chains, keys, PANIC media, presentations) at `f(layer)`.
    /// Slots and layer settings are moved by the caller.
    pub(crate) fn remap_layers(&mut self, f: impl Fn(usize) -> usize) {
        use crate::keymap::Action;
        let n = self.composition.layers.len();
        let f = |l: usize| (l < n).then(|| f(l));
        for d in &mut self.decks {
            for ch in &mut d.layer_chains {
                for s in &mut ch.steps {
                    s.layer = f(s.layer).unwrap_or(s.layer);
                }
                ch.steps.sort_by_key(|s| s.layer);
            }
        }
        self.keymap.remap(|a| match *a {
            Action::TriggerSlot { layer, col } => f(layer).map(|layer| Action::TriggerSlot { layer, col }),
            Action::ClearLayer(l) => f(l).map(Action::ClearLayer),
            ref a => Some(a.clone()),
        });
        self.panic_media = self.panic_media.and_then(|(d, l, c)| f(l).map(|l| (d, l, c)));
        for o in &mut self.outputs {
            if let crate::output::OutputSource::Layer(l) = &mut o.source {
                *l = f(*l).unwrap_or(*l);
            }
        }
        self.presentation_layer = self.presentation_layer.map(|l| f(l.min(n.saturating_sub(1))).unwrap_or(0));
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
        Project { composition: Composition { layers, ..Composition::default() }, decks: vec![deck], active_deck: 0, keymap: Default::default(), transitions: crate::transition::TransitionPreset::defaults(), outputs: vec![Default::default()], materi: Vec::new(), presentation_layer: None, panic_media: None, layer_order: 1 }
    }
}
