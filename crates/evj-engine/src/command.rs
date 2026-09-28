use evj_core::model::{Clip, Layer};
use std::sync::mpsc::Sender;

/// The presenter's digital pointer on a layer (0..1 coordinates).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pointer {
    pub x: f32,
    pub y: f32,
    /// Dim everything outside a circle instead of drawing a laser dot.
    pub spotlight: bool,
    /// Radius as a fraction of the picture height.
    pub size: f32,
}

/// Messages from the UI (or tests) to the engine thread. Processed in order, between frames.
pub enum Command {
    /// Opens `clip` in the background; the layer keeps showing its current clip until the new one has a frame.
    Trigger { layer: usize, clip: Clip, transition: Option<evj_core::transition::TransitionPreset> },
    /// Changes playback settings of the clip playing on `layer` (same file).
    UpdateClip { layer: usize, clip: Clip },
    /// Empties the layer; with a transition the current clip fades out.
    Clear { layer: usize, transition: Option<evj_core::transition::TransitionPreset> },
    /// Presentations (a `deck.json` triggered on the layer): next click / previous / jump to a slide.
    SlideNext { layer: usize, transition: Option<evj_core::transition::TransitionPreset> },
    SlidePrev { layer: usize, transition: Option<evj_core::transition::TransitionPreset> },
    SlideGoto { layer: usize, slide: usize, transition: Option<evj_core::transition::TransitionPreset> },
    /// Shows / moves / hides the presenter pointer on a layer.
    SetPointer { layer: usize, pointer: Option<Pointer> },
    SetLayer { layer: usize, props: Layer },
    SetLayerCount(usize),
    SetBpm(f64),
    /// Tap tempo (engine clock).
    Tap,
    /// Shift the beat phase by a fraction of a beat.
    Nudge(f64),
    /// Now is beat one.
    Resync,
    SetCompositionEffects(Vec<evj_core::effect::EffectRef>),
    /// Composition size (LED walls: e.g. 3840x1080). The preview texture is recreated: ask for it again.
    SetResolution { width: u32, height: u32 },
    /// Manual clock only: render one frame `dt` seconds after the previous one.
    Step { dt: f64 },
    /// Replies once every earlier command has been processed.
    Sync(Sender<()>),
    /// Replies with the composition as tight RGBA8.
    Readback(Sender<Vec<u8>>),
    /// Output window created by the UI thread; the engine owns its swapchain.
    OpenOutput { id: u32, hwnd: isize, width: u32, height: u32 },
    ResizeOutput { id: u32, width: u32, height: u32 },
    CloseOutput { id: u32 },
    /// What an output shows (source, slices, test pattern).
    SetOutputConfig { id: u32, config: evj_core::output::OutputConfig },
    /// Renders `config` offscreen and replies with RGBA8 pixels (UI preview, tests).
    RenderOutputPreview { config: evj_core::output::OutputConfig, width: u32, height: u32, reply: Sender<Vec<u8>> },
    /// Replies with a shared-texture handle (keyed mutex, key 0) holding a half-size composition preview.
    SharePreview(Sender<anyhow::Result<(isize, u32, u32)>>),
    /// Replies with a small shared texture (keyed mutex, key 0) that loops the selected transition.
    ShareTransitionPreview(Sender<anyhow::Result<(isize, u32, u32)>>),
    /// Transition shown in that preview (None = stop rendering it).
    PreviewTransition(Option<String>),
    /// Creates the operator's Preview monitor texture (cued clip); replies (handle, width, height).
    ShareCuePreview(Sender<anyhow::Result<(isize, u32, u32)>>),
    /// Shows `clip` (with its effects) on the Preview monitor only — never on the outputs, never heard.
    CueClip(Option<Clip>),
    /// The Preview monitor shows a whole scene: one clip per layer (index = layer).
    CueScene(Vec<Option<Clip>>),
    /// Pixels of the Preview monitor (RGBA, empty when there is none) — tests.
    ReadbackCue(Sender<Vec<u8>>),
    /// Pause / resume the cued clip on the Preview monitor.
    CuePause(bool),
    /// Cued clip back to its start (in point), paused.
    CueRewind,
    /// Moves a playing clip to `secs` (clamped to its start–end). `None` = the Preview cue.
    Seek { layer: Option<usize>, secs: f64 },
    /// Opens `clip` ahead and swaps it in (a cut) when the layer's clip finishes. None = forget
    /// it. A Trigger / Clear on the layer also forgets it.
    QueueNext { layer: usize, clip: Option<Clip> },
    /// Fade every output to black (true) or back (false) over 0.5 s.
    Blackout(bool),
    /// Turn the master CLIP lamp off.
    ResetClip,
    /// Test hook: behave as if the GPU driver had reset (TDR).
    SimulateDeviceLost,
    /// Master audio level (0..1).
    SetMasterVolume(f32),
    /// Fades all sound out fast.
    PanicAudio,
    /// Output device by name (None = Windows default).
    SetAudioDevice(Option<String>),
    /// Where a sound bus plays. Program `None` = Windows default, channels 1-2; Preview `None` = off.
    SetAudioRoute { bus: Bus, route: Option<evj_audio::Route> },
    /// Level of the Preview sound (0..2), the Preview output's master.
    SetPreviewVolume(f32),
    /// Restarts frame-time statistics (after warm-up).
    ResetStats,
    Shutdown,
}

/// The two sound paths: what the audience hears, and the operator's Preview (headphones).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bus {
    Program,
    Preview,
}
