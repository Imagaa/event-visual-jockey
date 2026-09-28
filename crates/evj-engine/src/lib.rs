//! The render engine: its own thread and D3D11 device, driven by [`Command`]s, reporting [`Snapshot`]s.
//! Nothing the UI does can block a frame here.
mod command;
pub mod fx;
mod layer;
mod output;
pub mod stats;
mod sys;

pub use command::{Bus, Command, Pointer};
pub use evj_media::DecoderKind;

use anyhow::{Context, Result, anyhow};
use evj_core::effect::{EffectRef, resolve};
use evj_core::model::{FitMode, Layer, PlayMode};
use evj_core::output::{OutputConfig, OutputSource};
use evj_core::tempo::Tempo;
use fx::{EffectInfo, Library};
use evj_render::{Blitter, Compositor, DeviceKind, FxParams, FxProgram, FxRunner, Gpu, RenderTarget, Shade, Texture, fit_rect};
use layer::LayerRt;
use output::Output;
use stats::FrameStats;
use std::collections::{HashMap, HashSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use windows::Win32::Foundation::S_OK;
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::core::Interface;

pub struct EngineConfig {
    pub device: DeviceKind,
    /// Frames only render on [`Command::Step`] (tests, offline).
    pub manual_clock: bool,
    pub width: u32,
    pub height: u32,
    pub layers: usize,
    /// Folders with user `.hlsl` effects (hot-reloaded).
    pub effect_folders: Vec<PathBuf>,
    /// Play clip sound through the default audio device.
    pub audio: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LayerState {
    pub clip_name: Option<String>,
    pub pos: f64,
    pub duration: f64,
    pub kind: Option<DecoderKind>,
    pub random_access: bool,
    pub has_frame: bool,
    pub finished: bool,
    pub loading: bool,
    pub error: Option<String>,
    /// Progress (0..1) of a running transition.
    pub transition: Option<f64>,
    /// The clip's sound is playing.
    pub audio: bool,
    pub audio_peak: f32,
    pub audio_peaks: [f32; 2],
    /// Seconds until the clip ends at its current speed (None: still image / unknown).
    pub remaining: Option<f64>,
    pub looping: bool,
    /// Presentation: (slide, click step, slide count).
    pub slide: Option<(usize, usize, usize)>,
    /// The part of the clip that plays (seconds): in and out point.
    pub start: f64,
    pub end: f64,
    pub clip_path: Option<PathBuf>,
    /// Queued clips swapped in so far (see `Command::QueueNext`).
    pub queue_taken: u64,
    /// Queued clips that failed to open (the current clip stays).
    pub queue_failed: u64,
    /// Voices of the clip playing now (its own sound, attached audio).
    pub voices: u8,
}

/// Transport of the clip on the Preview monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct CueState {
    pub name: String,
    pub pos: f64,
    pub duration: f64,
    pub remaining: Option<f64>,
    pub paused: bool,
    pub start: f64,
    pub end: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub adapter: String,
    pub layers: Vec<LayerState>,
    pub frames: u64,
    pub fps: f32,
    pub p99_ms: f32,
    pub dropped: u64,
    pub outputs: usize,
    pub resolution: (u32, u32),
    /// A shared preview texture exists (see `Command::SharePreview`).
    pub preview: bool,
    pub bpm: f64,
    pub beat: f64,
    /// Effect & transition library (built-ins + user folders).
    pub effects: Arc<Vec<EffectInfo>>,
    pub transition_preview: Option<String>,
    /// Name of the clip on the Preview monitor (None while nothing is cued / it is loading).
    pub cue: Option<String>,
    /// The cued clip's transport (Preview monitor).
    pub cue_state: Option<CueState>,
    /// Layers on the Preview monitor (1 = a clip, more = a scene).
    pub cue_layers: usize,
    pub master_peaks: [f32; 2],
    /// Output reached 0 dBFS since the last ResetClip.
    pub master_clip: bool,
    /// 0 = none … 1 = outputs fully black.
    pub blackout: f32,
    /// GPU device resets survived (driver TDR, device removed).
    pub device_resets: u32,
    /// Last frame times (ms), oldest first — for the performance graph.
    pub frame_times: Vec<f32>,
    /// GPU memory used by the engine (MB).
    pub gpu_memory_mb: f32,
    /// GPU time of the engine's frame (ms, average of the last second; 0 = not measured).
    pub gpu_ms: f32,
    /// GPU ms per stage: each layer, composition effects, each output, preview (recent average).
    pub gpu_stages: Vec<(String, f32)>,
    /// Audio output in use (None: no device / audio off).
    pub audio_device: Option<String>,
    pub master_peak: f32,
    /// Audio gaps (a playing voice ran out of decoded sound) since the output opened.
    pub audio_underruns: u64,
    /// Preview sound output (None: off / failed), its level meter, and why it is off.
    pub preview_audio_device: Option<String>,
    pub preview_peaks: [f32; 2],
    pub preview_audio_error: Option<String>,
}

pub struct Engine {
    tx: Sender<Command>,
    snapshot: Arc<Mutex<Snapshot>>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn start(cfg: EngineConfig) -> Result<Engine> {
        let (tx, rx) = channel();
        let (ready_tx, ready_rx) = channel();
        let snapshot = Arc::new(Mutex::new(Snapshot::default()));
        let snap = snapshot.clone();
        let thread = std::thread::Builder::new().name("evj-engine".into()).spawn(move || {
            let r = catch_unwind(AssertUnwindSafe(|| run(cfg, rx, &snap, &ready_tx)));
            if let Err(p) = r {
                let msg = p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned());
                let _ = ready_tx.send(Err(anyhow!("engine panicked: {}", msg.unwrap_or_default())));
            }
        })?;
        ready_rx.recv().map_err(|_| anyhow!("engine thread exited during start"))??;
        Ok(Engine { tx, snapshot, thread: Some(thread) })
    }

    pub fn send(&self, c: Command) {
        let _ = self.tx.send(c);
    }

    /// Manual clock: renders one frame and waits for it.
    pub fn step(&self, dt: f64) {
        self.send(Command::Step { dt });
        self.sync();
    }

    /// Waits until every command sent so far has been processed.
    pub fn sync(&self) {
        let (tx, rx) = channel();
        self.send(Command::Sync(tx));
        let _ = rx.recv_timeout(Duration::from_secs(10));
    }

    pub fn snapshot(&self) -> Snapshot {
        self.snapshot.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub fn shutdown(mut self) -> Result<()> {
        self.stop()
    }

    fn stop(&mut self) -> Result<()> {
        self.send(Command::Shutdown);
        if let Some(t) = self.thread.take() {
            t.join().map_err(|_| anyhow!("engine thread panicked"))?;
        }
        Ok(())
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

struct Preview {
    rt: RenderTarget,
    mutex: IDXGIKeyedMutex,
}

struct State {
    gpu: Gpu,
    comp: Compositor,
    blitter: Blitter,
    layers: Vec<LayerRt>,
    props: Vec<Layer>,
    tempo: Tempo,
    /// Engine clock (seconds), drives tap tempo and effect TIME.
    time: f64,
    outputs: Vec<Output>,
    preview: Option<Preview>,
    stats: FrameStats,
    frames: u64,
    lib: Library,
    lib_version: u64,
    infos: Arc<Vec<EffectInfo>>,
    last_scan: Instant,
    fx: FxRunner,
    /// Previous outputs of feedback effects, keyed by chain position.
    histories: HashMap<String, RenderTarget>,
    comp_effects: Vec<EffectRef>,
    /// What the outputs showed last frame (composition after its effects).
    last_out: Option<Texture>,
    /// Holds the outgoing clip's image while the incoming one is prepared.
    stash: RenderTarget,
    /// 1x1 transparent: the destination when a layer fades out.
    clear_tex: Texture,
    /// Transition Manager preview: selected transition + its two test cards.
    tr_preview: Option<(String, [Texture; 2])>,
    tr_preview_rt: Option<Preview>,
    /// The cued clip for the Preview monitor, and its shared texture.
    /// One entry for a cued clip, one per layer for a cued scene.
    cue: Vec<LayerRt>,
    cue_scene: bool,
    /// Blends a scene for the Preview monitor (at its size).
    cue_comp: Option<Compositor>,
    cue_rt: Option<Preview>,
    /// BLACKOUT: current level (0..1) and where it is heading.
    blackout: f32,
    blackout_on: bool,
    black_tex: Texture,
    /// Per-layer images (before blending) for outputs that show a single layer.
    taps: HashMap<usize, RenderTarget>,
    /// Layers some output (or preview request) wants a tap of.
    wanted_taps: HashSet<usize>,
    kind: DeviceKind,
    effect_folders: Vec<PathBuf>,
    simulate_lost: bool,
    device_resets: u32,
    gpu_memory_mb: f32,
    audio: Option<evj_audio::AudioEngine>,
    next_voice: u64,
    master_volume: f32,
    /// The Preview sound output (the cue layers' voices) and its level.
    preview_audio: Option<evj_audio::AudioEngine>,
    preview_volume: f32,
    preview_audio_error: Option<String>,
    pointers: HashMap<usize, Pointer>,
    timer: Option<evj_render::GpuTimer>,
    gpu_times: std::collections::VecDeque<f32>,
    gpu_stages: Vec<(String, f32)>,
    stage_names: Vec<String>,
}

fn run(cfg: EngineConfig, rx: Receiver<Command>, snap: &Mutex<Snapshot>, ready: &Sender<Result<()>>) {
    let st = Gpu::new(cfg.device).and_then(|gpu| {
        Ok(State {
            comp: Compositor::new(&gpu, cfg.width, cfg.height)?,
            blitter: Blitter::new(&gpu)?,
            layers: (0..cfg.layers).map(|_| LayerRt::default()).collect(),
            props: vec![Layer::default(); cfg.layers],
            tempo: Tempo::new(120.0),
            time: 0.0,
            outputs: Vec::new(),
            preview: None,
            stats: FrameStats::default(),
            frames: 0,
            lib: Library::new(&gpu, cfg.effect_folders.clone()),
            lib_version: u64::MAX,
            infos: Arc::new(Vec::new()),
            last_scan: Instant::now(),
            fx: FxRunner::new(&gpu, cfg.width, cfg.height)?,
            histories: HashMap::new(),
            comp_effects: Vec::new(),
            last_out: None,
            stash: RenderTarget::new(&gpu, cfg.width, cfg.height)?,
            clear_tex: {
                let t = Texture::new_color(&gpu, windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM, 1, 1)?;
                t.upload(&gpu.ctx, &[0, 0, 0, 0], 4);
                t
            },
            tr_preview: None,
            tr_preview_rt: None,
            cue: vec![LayerRt::default()],
            cue_scene: false,
            cue_comp: None,
            cue_rt: None,
            blackout: 0.0,
            blackout_on: false,
            black_tex: {
                let t = Texture::new_color(&gpu, windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM, 1, 1)?;
                t.upload(&gpu.ctx, &[0, 0, 0, 255], 4);
                t
            },
            taps: HashMap::new(),
            wanted_taps: HashSet::new(),
            kind: cfg.device,
            effect_folders: cfg.effect_folders.clone(),
            simulate_lost: false,
            device_resets: 0,
            gpu_memory_mb: 0.0,
            audio: if cfg.audio {
                evj_audio::AudioEngine::start(None).map_err(|e| evj_core::log::warn("audio", &format!("no audio output: {e:#}"))).ok()
            } else {
                None
            },
            next_voice: 0,
            master_volume: 1.0,
            preview_audio: None,
            preview_volume: 1.0,
            preview_audio_error: None,
            pointers: HashMap::new(),
            timer: evj_render::GpuTimer::new(&gpu).ok(),
            gpu_times: std::collections::VecDeque::new(),
            gpu_stages: Vec::new(),
            stage_names: Vec::new(),
            gpu,
        })
    });
    let mut st = match st {
        Ok(st) => st,
        Err(e) => {
            let _ = ready.send(Err(e));
            return;
        }
    };
    let _ = ready.send(Ok(()));
    if !cfg.manual_clock {
        sys::tune_engine_thread();
    }
    let mut awake = false;

    let mut last = Instant::now();
    'frames: loop {
        if cfg.manual_clock {
            let Ok(cmd) = rx.recv() else { break };
            match cmd {
                Command::Shutdown => break,
                Command::Step { dt } => st.tick(dt),
                c => st.handle(c),
            }
        } else {
            // Paced by the first output's vblank; by a 60 Hz timer when there is none or it is
            // covered / minimised (its Present returns at once and would spin the GPU).
            if !st.outputs.first().is_some_and(|o| o.wait()) {
                let next = last + Duration::from_micros(16_667);
                std::thread::sleep(next.saturating_duration_since(Instant::now()));
            }
            while let Ok(cmd) = rx.try_recv() {
                match cmd {
                    Command::Shutdown => break 'frames,
                    c => st.handle(c),
                }
            }
            let now = Instant::now();
            let dt = (now - last).as_secs_f64();
            last = now;
            st.stats.record(dt as f32 * 1000.0, 1000.0 / 60.0);
            st.tick(dt);
        }
        if let Ok(mut s) = snap.lock() {
            *s = st.snapshot();
        }
        if !cfg.manual_clock && awake != !st.outputs.is_empty() {
            awake = !st.outputs.is_empty();
            sys::keep_awake(awake);
        }
    }
}

/// Runs effect chains for one frame; remembers which feedback histories are still in use.
struct Chains<'a> {
    gpu: &'a Gpu,
    fx: &'a mut FxRunner,
    lib: &'a Library,
    histories: &'a mut HashMap<String, RenderTarget>,
    used: HashSet<String>,
    time: f64,
    beat: f64,
}

/// A layer's picture whose last step has not been drawn yet, so it can go straight onto the
/// composition with a fixed-function blend (no scratch target, no separate blend pass).
enum Pending<'a> {
    Ready(Texture),
    /// The clip itself, drawn into `rect` of the composition.
    Clip { tex: &'a Texture, shade: Shade, rect: [f32; 4] },
    /// One effect / transition pass.
    Pass { program: &'a FxProgram, input: Texture, second: Option<Texture>, params: FxParams },
}

impl<'a> Chains<'a> {
    /// Draws whatever `p` still waits for (into a scratch target, or not at all).
    fn materialize(&mut self, comp: &mut Compositor, p: Pending) -> Texture {
        match p {
            Pending::Ready(t) => t,
            Pending::Clip { tex, shade, rect } => {
                // A clip that already covers the composition as plain RGBA (HAP / image at
                // composition size) is used as is; effects then read the small compressed texture.
                let (w, h) = (comp.width, comp.height);
                if shade == Shade::Rgba && tex.width == w && tex.height == h && rect == [0.0, 0.0, w as f32, h as f32] {
                    tex.clone()
                } else {
                    comp.prepare(&self.gpu.ctx, tex, shade, rect)
                }
            }
            Pending::Pass { program, input, second, params } => self.fx.pass(self.gpu, program, &input, second.as_ref(), &params),
        }
    }

    fn apply(&mut self, comp: &mut Compositor, scope: &str, chain: &[EffectRef], input: Texture) -> Texture {
        let p = self.apply_pending(comp, scope, chain, Pending::Ready(input));
        self.materialize(comp, p)
    }

    /// Runs `chain`; its last plain (non-feedback) effect is left pending.
    fn apply_pending<'p>(&mut self, comp: &mut Compositor, scope: &str, chain: &[EffectRef], input: Pending<'p>) -> Pending<'p>
    where
        'a: 'p,
    {
        let mut cur = input;
        for (i, e) in chain.iter().enumerate() {
            let lib: &'a Library = self.lib;
            let Some(entry) = lib.find(&e.name).filter(|_| !e.bypass) else { continue };
            let Some(program) = &entry.program else { continue };
            let params = FxParams { time: self.time as f32, beat: self.beat as f32, progress: 0.0, values: resolve(&entry.meta, e, self.beat) };
            let t = self.materialize(comp, cur);
            if !entry.meta.feedback {
                cur = Pending::Pass { program, input: t, second: None, params };
                continue;
            }
            let key = format!("{scope}/{i}/{}", e.name);
            if !self.histories.contains_key(&key) {
                let Ok(rt) = RenderTarget::new(self.gpu, self.fx.width, self.fx.height) else {
                    cur = Pending::Ready(t);
                    continue;
                };
                unsafe { self.gpu.ctx.ClearRenderTargetView(&rt.rtv, &[0.0, 0.0, 0.0, 0.0]) };
                self.histories.insert(key.clone(), rt);
            }
            let prev = self.histories.get(&key).map(RenderTarget::as_texture);
            let out = self.fx.pass(self.gpu, program, &t, prev.as_ref(), &params);
            if let Some(h) = self.histories.get(&key) {
                self.fx.keep(self.gpu, &out, h);
            }
            self.used.insert(key);
            cur = Pending::Ready(out);
        }
        cur
    }
}

/// Test cards for the transition preview: warm stripes (A) and a cool checker (B).
fn preview_card(gpu: &Gpu, b: bool) -> Result<Texture> {
    let (w, h) = (256u32, 144u32);
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let c = if b {
                let on = ((x / 32) + (y / 32)) % 2 == 0;
                if on { [40, 120, 230, 255] } else { [20, 200, 200, 255] }
            } else {
                let stripe = (x / 16) % 2 == 0;
                let r = 150 + (x * 100 / w) as u8;
                if stripe { [r, 70, 40, 255] } else { [r, 30, 120, 255] }
            };
            px.extend_from_slice(&c);
        }
    }
    let t = Texture::new_color(gpu, windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM, w, h)?;
    t.upload(&gpu.ctx, &px, w * 4);
    Ok(t)
}

fn fit_index(f: FitMode) -> u32 {
    match f {
        FitMode::Fit => 0,
        FitMode::Fill => 1,
        FitMode::Stretch => 2,
    }
}

/// The layer's sounds are on an output that is gone: they start again on the next update.
fn forget_voices(l: &mut LayerRt) {
    if let Some(a) = l.active.as_mut() {
        (a.voice, a.audio_started, a.audio_stopping, a.gain_sent) = (None, false, false, -1.0);
        (a.attached_voice, a.attached_started, a.attached_stopping, a.attached_gain) = (None, false, false, -1.0);
    }
}

/// Voices no layer refers to any more (cleared layer, cancelled transition, removed layer) fade out.
fn stop_unreferenced(audio: &mut evj_audio::AudioEngine, referenced: &HashSet<u64>) {
    for id in audio.playing_ids() {
        if !referenced.contains(&id) {
            audio.stop(id, 0.05);
        }
    }
}

/// Program and Preview route clash: the same device (Windows default resolved) and pair.
fn clashes(program: &evj_audio::AudioEngine, route: &evj_audio::Route) -> bool {
    let wanted = route.device.clone().or_else(evj_audio::AudioEngine::default_device);
    wanted.as_deref() == Some(program.device()) && program.route().first_channel == route.first_channel
}

/// One layer's sounds on `audio`: its own voice and the attached audio, started where the
/// picture is, following seek / pause / finish / transitions, at `target` gain.
fn drive_voices(audio: &mut evj_audio::AudioEngine, l: &mut LayerRt, target: f32, next_voice: &mut u64, referenced: &mut HashSet<u64>) {
    let rate = audio.rate();
    let fade = l.outgoing.as_ref().map(|(_, run)| run.remaining() as f32);
    if let Some((o, run)) = l.outgoing.as_mut() {
        if let Some(v) = &o.voice {
            referenced.insert(v.id);
            if !o.audio_stopping {
                audio.stop(v.id, run.remaining() as f32);
                o.audio_stopping = true;
            }
        }
        if let Some(v) = &o.attached_voice {
            referenced.insert(v.id);
            if !o.attached_stopping {
                audio.stop(v.id, run.remaining() as f32);
                o.attached_stopping = true;
            }
        }
    }
    let Some(a) = l.active.as_mut() else { return };
    // Seek / A–B loop / pause: the sound starts again where the picture is.
    if std::mem::take(&mut a.audio_restart) || a.paused {
        if let Some(v) = a.voice.take() {
            audio.stop(v.id, 0.02);
        }
        if let Some(v) = a.attached_voice.take() {
            audio.stop(v.id, 0.02);
        }
        a.audio_started = false;
        a.audio_stopping = false;
        a.attached_started = false;
        a.attached_stopping = false;
    }
    if a.paused {
        return; // a held Preview is silent
    }
    // Out point before the file's end (Once): the sound stops with the picture.
    if a.finished() && !a.audio_stopping {
        if let Some(v) = &a.voice {
            audio.stop(v.id, 0.05);
            a.audio_stopping = true;
        }
    }
    if a.finished() && !a.attached_stopping {
        if let Some(v) = &a.attached_voice {
            audio.stop(v.id, 0.05);
            a.attached_stopping = true;
        }
    }
    if !a.audio_started && a.audible() && !a.finished() {
        a.audio_started = true;
        *next_voice += 1;
        let (id, start) = (*next_voice, a.clip_time());
        let initial = if fade.is_some() { 0.0 } else { target };
        match audio.play(id, &a.clip.path, initial, start, a.loops_whole_file()) {
            Ok(h) => {
                (a.voice, a.audio_start, a.audio_rate, a.gain_sent) = (Some(h), start, rate, initial);
                if let Some(f) = fade {
                    audio.set_gain(id, target, f); // crossfade with the outgoing clip
                    a.gain_sent = target;
                }
            }
            Err(e) => evj_core::log::warn("audio", &format!("{}: {e:#}", a.clip.name)),
        }
    }
    if let Some(v) = &a.voice {
        referenced.insert(v.id);
        if a.gain_sent != target {
            audio.set_gain(v.id, target, 0.05);
            a.gain_sent = target;
        }
    }
    // Attached audio: starts with the clip (clip time since the in point).
    let attached = a.clip.attached.clone().filter(|x| !x.path.as_os_str().is_empty() && a.player.info.kind != DecoderKind::Audio);
    if let Some(at) = attached {
        let gain = target * at.volume.clamp(0.0, 2.0);
        if !a.attached_started && !a.finished() {
            a.attached_started = true;
            *next_voice += 1;
            let (len, looping) = (a.attached_secs, a.clip.mode == PlayMode::Loop);
            let t = a.clip_time() - a.range().0; // stills: their own clock
            let start = if len > 0.0 && looping { t.rem_euclid(len) } else { t.max(0.0) };
            let initial = if fade.is_some() { 0.0 } else { gain };
            match audio.play(*next_voice, &at.path, initial, start, looping) {
                Ok(h) => {
                    (a.attached_voice, a.attached_start, a.audio_rate, a.attached_gain) = (Some(h), start, rate, initial);
                }
                Err(e) => evj_core::log::warn("audio", &format!("{}: {e:#}", at.path.display())),
            }
        }
        if let Some(v) = &a.attached_voice {
            referenced.insert(v.id);
            if a.attached_gain != gain {
                audio.set_gain(v.id, gain, fade.unwrap_or(0.05));
                a.attached_gain = gain;
            }
            if let Some(e) = v.error().filter(|_| !a.attached_error_shown) {
                a.attached_error_shown = true;
                l.error = Some(format!("attached audio: {e}"));
            }
        }
    }
}

/// Cues `clip` on one Preview layer (same file: only its settings changed).
fn cue_one(l: &mut LayerRt, device: &windows::Win32::Graphics::Direct3D11::ID3D11Device, clip: Option<evj_core::model::Clip>) {
    match clip {
        Some(c) if l.active.as_ref().is_some_and(|a| a.clip.path == c.path) => l.update_clip(c),
        Some(c) if evj_core::slides::SlideDeck::is_deck(&c.path) => l.open_deck(device, c, None),
        Some(c) => l.trigger(device, c, None),
        None => *l = LayerRt::default(),
    }
}

impl State {
    fn handle(&mut self, cmd: Command) {
        match cmd {
            Command::Trigger { layer, clip, transition } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    if evj_core::slides::SlideDeck::is_deck(&clip.path) {
                        l.open_deck(&self.gpu.device, clip, transition);
                    } else {
                        l.trigger(&self.gpu.device, clip, transition);
                    }
                }
            }
            Command::SlideNext { layer, transition } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.slide_next(&self.gpu.device, transition);
                }
            }
            Command::SlidePrev { layer, transition } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.slide_prev(&self.gpu.device, transition);
                }
            }
            Command::SlideGoto { layer, slide, transition } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.slide_goto(&self.gpu.device, slide, transition);
                }
            }
            Command::UpdateClip { layer, clip } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.update_clip(clip);
                }
            }
            Command::Clear { layer, transition } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.clear(transition, self.tempo.bpm);
                }
            }
            Command::SetLayer { layer, props } => {
                if let Some(p) = self.props.get_mut(layer) {
                    *p = props;
                }
            }
            Command::SetLayerCount(n) => {
                self.layers.resize_with(n, LayerRt::default);
                self.props.resize(n, Layer::default());
            }
            Command::SetBpm(bpm) => self.tempo.set_bpm(bpm),
            Command::Tap => self.tempo.tap(self.time),
            Command::Nudge(beats) => self.tempo.nudge(beats),
            Command::Resync => self.tempo.resync(),
            Command::SetCompositionEffects(chain) => self.comp_effects = chain,
            Command::SetResolution { width, height } => {
                let (w, h) = (width.clamp(1, 16384), height.clamp(1, 16384));
                if (w, h) == (self.comp.width, self.comp.height) {
                    return;
                }
                match Compositor::new(&self.gpu, w, h).and_then(|c| Ok((c, FxRunner::new(&self.gpu, w, h)?, RenderTarget::new(&self.gpu, w, h)?))) {
                    Ok((c, fx, stash)) => {
                        self.comp = c;
                        self.fx = fx;
                        self.stash = stash;
                        self.histories.clear();
                        self.last_out = None;
                        self.preview = None;
                    }
                    Err(e) => evj_core::log::warn("engine", &format!("resolution {w}x{h}: {e:#}")),
                }
            }
            Command::Step { .. } => {}
            Command::Sync(tx) => {
                let _ = tx.send(());
            }
            Command::Readback(tx) => {
                let out = self.last_out.clone().unwrap_or_else(|| self.comp.output().as_texture());
                let _ = tx.send(out.readback(&self.gpu).unwrap_or_default());
            }
            Command::OpenOutput { id, hwnd, width, height } => match Output::new(&self.gpu, id, hwnd, width, height) {
                Ok(o) => self.outputs.push(o),
                Err(e) => evj_core::log::warn("engine", &format!("output {id}: {e:#}")),
            },
            Command::ResizeOutput { id, width, height } => {
                if let Some(o) = self.outputs.iter_mut().find(|o| o.id == id) {
                    let _ = o.resize(&self.gpu, width, height);
                }
            }
            Command::CloseOutput { id } => self.outputs.retain(|o| o.id != id),
            Command::SetOutputConfig { id, config } => {
                if let Some(o) = self.outputs.iter_mut().find(|o| o.id == id) {
                    o.config = config;
                }
            }
            Command::RenderOutputPreview { config, width, height, reply } => {
                if let OutputSource::Layer(n) = config.source {
                    self.wanted_taps.insert(n);
                }
                let px = RenderTarget::new(&self.gpu, width.max(1), height.max(1)).and_then(|rt| {
                    let out = self.last_out.clone().unwrap_or_else(|| self.comp.output().as_texture());
                    let out = if config.test_pattern { self.park(out) } else { out };
                    let src = self.output_source(&config, &out, 0);
                    output::draw_slices(&self.gpu, &self.blitter, &src, &config.slices, &rt.rtv, rt.width, rt.height);
                    rt.readback(&self.gpu)
                });
                let _ = reply.send(px.unwrap_or_default());
            }
            Command::SharePreview(tx) => {
                let (w, h) = ((self.comp.width / 2).max(1), (self.comp.height / 2).max(1));
                let r = RenderTarget::new_shared(&self.gpu, w, h).and_then(|(rt, handle)| {
                    let mutex = rt.tex.cast::<IDXGIKeyedMutex>().context("keyed mutex")?;
                    self.preview = Some(Preview { rt, mutex });
                    Ok((handle, w, h))
                });
                let _ = tx.send(r);
            }
            Command::ShareTransitionPreview(tx) => {
                let (w, h) = (256, 144);
                let r = RenderTarget::new_shared(&self.gpu, w, h).and_then(|(rt, handle)| {
                    let mutex = rt.tex.cast::<IDXGIKeyedMutex>().context("keyed mutex")?;
                    self.tr_preview_rt = Some(Preview { rt, mutex });
                    Ok((handle, w, h))
                });
                let _ = tx.send(r);
            }
            Command::ShareCuePreview(tx) => {
                let (w, h) = (640, (640 * self.comp.height / self.comp.width.max(1)).max(1));
                let r = RenderTarget::new_shared(&self.gpu, w, h).and_then(|(rt, handle)| {
                    let mutex = rt.tex.cast::<IDXGIKeyedMutex>().context("keyed mutex")?;
                    self.cue_rt = Some(Preview { rt, mutex });
                    self.cue_comp = None;
                    Ok((handle, w, h))
                });
                let _ = tx.send(r);
            }
            Command::CueClip(clip) => {
                if self.cue_scene {
                    self.cue = vec![LayerRt::default()];
                    self.cue_scene = false;
                }
                cue_one(&mut self.cue[0], &self.gpu.device, clip);
            }
            Command::CueScene(clips) => {
                if !self.cue_scene {
                    self.cue.clear();
                    self.cue_scene = true;
                }
                self.cue.resize_with(clips.len(), LayerRt::default);
                for (l, c) in self.cue.iter_mut().zip(clips) {
                    cue_one(l, &self.gpu.device, c);
                }
            }
            Command::ReadbackCue(tx) => {
                let px = self.cue_rt.as_ref().and_then(|p| {
                    if unsafe { (Interface::vtable(&p.mutex).AcquireSync)(Interface::as_raw(&p.mutex), 0, 0) } != S_OK {
                        return None;
                    }
                    // The shared target is BGRA: copy it into an RGBA target first.
                    let px = RenderTarget::new(&self.gpu, p.rt.width, p.rt.height).ok().and_then(|tmp| {
                        let full = [0.0, 0.0, p.rt.width as f32, p.rt.height as f32];
                        self.blitter.draw(&self.gpu.ctx, &p.rt.as_texture(), Shade::Rgba, &tmp.rtv, full);
                        tmp.readback(&self.gpu).ok()
                    });
                    unsafe {
                        let _ = p.mutex.ReleaseSync(0);
                    }
                    px
                });
                let _ = tx.send(px.unwrap_or_default());
            }
            Command::CuePause(p) => {
                for a in self.cue.iter_mut().filter_map(|l| l.active.as_mut()) {
                    a.paused = p;
                }
            }
            Command::CueRewind => {
                for l in &mut self.cue {
                    let Some(clip) = l.active.as_ref().map(|a| a.clip.clone()) else { continue };
                    if l.active.as_ref().is_some_and(|a| a.player.info.random_access) {
                        if let Some(a) = l.active.as_mut() {
                            a.rewind();
                            a.paused = true;
                        }
                    } else {
                        // Sequential decoders cannot seek back: open the clip again, paused.
                        l.start_paused = true;
                        l.trigger(&self.gpu.device, clip, None);
                    }
                }
            }
            Command::Seek { layer: Some(l), secs } => {
                if let Some(l) = self.layers.get_mut(l) {
                    l.seek(&self.gpu.device, secs);
                }
            }
            Command::Seek { layer: None, secs } => {
                for l in &mut self.cue {
                    l.seek(&self.gpu.device, secs);
                }
            }
            Command::QueueNext { layer, clip } => {
                if let Some(l) = self.layers.get_mut(layer) {
                    l.queue(&self.gpu.device, clip);
                }
            }
            Command::Blackout(on) => self.blackout_on = on,
            Command::ResetClip => {
                if let Some(a) = self.audio.as_ref() {
                    a.reset_clip();
                }
            }
            Command::PreviewTransition(name) => {
                let cards = match self.tr_preview.take() {
                    Some((_, cards)) => Some(cards),
                    None => preview_card(&self.gpu, false).and_then(|a| Ok([a, preview_card(&self.gpu, true)?])).ok(),
                };
                self.tr_preview = name.filter(|_| self.tr_preview_rt.is_some()).zip(cards);
            }
            Command::SetPointer { layer, pointer } => match pointer {
                Some(p) => {
                    self.pointers.insert(layer, p);
                }
                None => {
                    self.pointers.remove(&layer);
                }
            },
            Command::SimulateDeviceLost => self.simulate_lost = true,
            Command::SetMasterVolume(v) => {
                self.master_volume = v.clamp(0.0, 1.0);
                if let Some(a) = self.audio.as_mut() {
                    a.master(self.master_volume, 0.05);
                }
            }
            Command::PanicAudio => {
                if let Some(a) = self.audio.as_mut() {
                    a.panic(1.0);
                }
                if let Some(a) = self.preview_audio.as_mut() {
                    a.panic(1.0);
                }
                self.master_volume = 0.0;
                self.preview_volume = 0.0;
            }
            Command::SetAudioDevice(name) => {
                let route = name.map(|device| evj_audio::Route { device: Some(device), first_channel: 0 });
                self.handle(Command::SetAudioRoute { bus: Bus::Program, route });
            }
            Command::SetAudioRoute { bus: Bus::Program, route } => match evj_audio::AudioEngine::start_route(&route.unwrap_or_default(), true) {
                Ok(mut a) => {
                    a.master(self.master_volume, 0.0);
                    self.audio = Some(a);
                    // Sounds restart on the new device where the pictures are now.
                    for l in &mut self.layers {
                        forget_voices(l);
                    }
                }
                Err(e) => evj_core::log::warn("audio", &format!("cannot open audio device: {e:#}")),
            },
            Command::SetAudioRoute { bus: Bus::Preview, route } => {
                self.preview_audio = None;
                self.preview_audio_error = None;
                for l in &mut self.cue {
                    forget_voices(l);
                }
                let Some(r) = route else { return };
                if self.audio.as_ref().is_some_and(|p| clashes(p, &r)) {
                    self.preview_audio_error = Some("Preview would play on the Program output — pick another device or channels".into());
                    return;
                }
                match evj_audio::AudioEngine::start_route(&r, false) {
                    Ok(mut a) => {
                        a.master(self.preview_volume, 0.0);
                        self.preview_audio = Some(a);
                    }
                    Err(e) => self.preview_audio_error = Some(format!("{e:#}")),
                }
            }
            Command::SetPreviewVolume(v) => {
                self.preview_volume = v.clamp(0.0, 2.0);
                if let Some(a) = self.preview_audio.as_mut() {
                    a.master(self.preview_volume, 0.05);
                }
            }
            Command::ResetStats => self.stats.reset(),
            Command::Shutdown => {}
        }
    }

    fn tick(&mut self, dt: f64) {
        if self.simulate_lost || unsafe { self.gpu.device.GetDeviceRemovedReason() }.is_err() {
            self.simulate_lost = false;
            if let Err(e) = self.recover() {
                evj_core::log::warn("engine", &format!("GPU recovery failed, retrying: {e:#}"));
                return;
            }
        }
        self.time += dt;
        self.tempo.advance(dt);
        let step = (dt / 0.5) as f32;
        self.blackout = if self.blackout_on { (self.blackout + step).min(1.0) } else { (self.blackout - step).max(0.0) };
        if self.last_scan.elapsed() > Duration::from_secs(1) {
            self.last_scan = Instant::now();
            self.lib.rescan(&self.gpu);
            self.gpu_memory_mb = self.gpu.memory_mb().unwrap_or(0.0);
        }
        if self.lib.version != self.lib_version {
            self.lib_version = self.lib.version;
            self.infos = Arc::new(self.lib.infos());
        }
        self.update_audio();
        for l in &mut self.layers {
            l.update(&self.gpu, dt, self.tempo.bpm);
            // A transition whose shader is missing or broken is a cut.
            let missing = l.outgoing.as_ref().is_some_and(|(_, run)| self.lib.find_transition(&run.preset.shader).and_then(|e| e.program.as_ref()).is_none());
            if missing {
                l.outgoing = None;
            }
        }
        if self.cue_rt.is_some() {
            for l in &mut self.cue {
                l.update(&self.gpu, dt, self.tempo.bpm);
            }
        }
        let (time, beat) = (self.time, self.tempo.beat);
        let mut chains = Chains { gpu: &self.gpu, fx: &mut self.fx, lib: &self.lib, histories: &mut self.histories, used: HashSet::new(), time, beat };
        let ctx = &self.gpu.ctx;
        let timing = self.timer.as_mut().is_some_and(|t| t.begin(ctx));
        self.comp.begin(ctx);
        let any_solo = self.props.iter().any(|p| p.solo);
        let mut stages: Vec<String> = Vec::new();
        for (i, (l, p)) in self.layers.iter().zip(&self.props).enumerate() {
            if timing {
                if let Some(t) = self.timer.as_mut() {
                    t.mark(ctx);
                    stages.push(format!("layer {}", i + 1));
                }
            }
            if p.bypass || (any_solo && !p.solo) {
                continue;
            }
            let (w, h) = (self.comp.width, self.comp.height);
            let full = [0.0, 0.0, w as f32, h as f32];
            fn clip(a: &layer::Active, w: u32, h: u32) -> Option<Pending<'_>> {
                let tex = a.tex.as_ref()?;
                Some(Pending::Clip { tex, shade: a.shade, rect: fit_rect(tex.width, tex.height, w, h, fit_index(a.clip.fit)) })
            }
            // The outgoing image is parked in `stash` before the incoming one reuses the scratch targets.
            let from = match l.outgoing.as_ref().and_then(|(a, run)| Some((clip(a, w, h)?, a, run))) {
                Some((c, a, run)) => {
                    let p = chains.apply_pending(&mut self.comp, &format!("out{i}"), &a.clip.effects, c);
                    let t = chains.materialize(&mut self.comp, p);
                    // A blit, not CopyResource: `t` may be the clip's own (compressed) texture.
                    self.blitter.draw(ctx, &t, Shade::Rgba, &self.stash.rtv, full);
                    Some((self.stash.as_texture(), run))
                }
                None => None,
            };
            let to = l.active.as_ref().and_then(|a| Some(chains.apply_pending(&mut self.comp, &format!("clip{i}"), &a.clip.effects, clip(a, w, h)?)));
            let t = match (from, to) {
                (Some((from, run)), to) => {
                    let entry = self.lib.find_transition(&run.preset.shader);
                    let Some((entry, program)) = entry.and_then(|e| e.program.as_ref().map(|p| (e, p))) else { continue };
                    let params = EffectRef { name: run.preset.shader.clone(), bypass: false, params: run.preset.params.clone() };
                    let fx = FxParams { time: time as f32, beat: beat as f32, progress: run.progress() as f32, values: resolve(&entry.meta, &params, beat) };
                    let to = match to {
                        Some(p) => chains.materialize(&mut self.comp, p),
                        None => self.clear_tex.clone(),
                    };
                    Pending::Pass { program, input: from, second: Some(to), params: fx }
                }
                (None, Some(to)) => to,
                (None, None) => continue,
            };
            let mut chain = p.effects.clone();
            if let Some(ptr) = self.pointers.get(&i) {
                let mut e = EffectRef::new("Pointer");
                for (name, v) in [("px", ptr.x), ("py", ptr.y), ("spotlight", if ptr.spotlight { 1.0 } else { 0.0 }), ("size", ptr.size)] {
                    e.param_mut(name, 0.0).value = v as f64;
                }
                chain.push(e);
            }
            let t = chains.apply_pending(&mut self.comp, &format!("layer{i}"), &chain, t);
            let tapped = self.wanted_taps.contains(&i);
            if tapped && !self.taps.contains_key(&i) {
                if let Ok(rt) = RenderTarget::new(&self.gpu, w, h) {
                    self.taps.insert(i, rt);
                }
            }
            // A tapped layer with a fixed blend: the last step draws into the tap, and the tap is
            // blended onto the composition (two passes instead of three).
            let tap_fixed = self.taps.get(&i).filter(|_| tapped).zip(self.comp.fixed(p.blend.index()));
            if let Some((tap, (rtv, state))) = tap_fixed {
                unsafe { ctx.ClearRenderTargetView(&tap.rtv, &[0.0, 0.0, 0.0, 0.0]) };
                match t {
                    Pending::Clip { tex, shade, rect } => self.blitter.draw(ctx, tex, shade, &tap.rtv, rect),
                    Pending::Ready(t) => self.blitter.draw(ctx, &t, Shade::Rgba, &tap.rtv, full),
                    Pending::Pass { program, input, second, params } => chains.fx.pass_to(&self.gpu, program, &input, second.as_ref(), &params, &tap.rtv),
                }
                self.blitter.draw_blend(ctx, &tap.as_texture(), Shade::Rgba, &rtv, full, &state, p.opacity);
                continue;
            }
            // Alpha / Screen / Multiply: the last step draws straight onto the composition.
            match self.comp.fixed(p.blend.index()).filter(|_| !tapped) {
                Some((rtv, state)) => match t {
                    Pending::Clip { tex, shade, rect } => self.blitter.draw_blend(ctx, tex, shade, &rtv, rect, &state, p.opacity),
                    Pending::Ready(t) => self.blitter.draw_blend(ctx, &t, Shade::Rgba, &rtv, full, &state, p.opacity),
                    Pending::Pass { program, input, second, params } => chains.fx.pass_blend(&self.gpu, program, &input, second.as_ref(), &params, &rtv, &state, p.opacity),
                },
                None => {
                    let t = chains.materialize(&mut self.comp, t);
                    if tapped {
                        if let Some(tap) = self.taps.get(&i) {
                            // A blit, not CopyResource: `t` may be the clip's own (compressed) texture.
                            self.blitter.draw(ctx, &t, Shade::Rgba, &tap.rtv, full);
                        }
                    }
                    self.comp.blend(ctx, &t, p.blend.index(), p.opacity);
                }
            }
        }
        if timing {
            if let Some(t) = self.timer.as_mut() {
                t.mark(ctx);
                stages.push("composition fx".into());
            }
        }
        let out = self.comp.output().as_texture();
        let out = chains.apply(&mut self.comp, "comp", &self.comp_effects, out);
        // BLACKOUT: the composition darkened towards black (outputs and the Program monitor see it).
        let out = if self.blackout > 0.0 {
            let full = [0.0, 0.0, self.comp.width as f32, self.comp.height as f32];
            if out.tex != self.stash.tex {
                self.blitter.draw(ctx, &out, Shade::Rgba, &self.stash.rtv, full);
            }
            if let Some((_, alpha)) = self.comp.fixed(0) {
                self.blitter.draw_blend(ctx, &self.black_tex, Shade::Rgba, &self.stash.rtv, full, &alpha, self.blackout);
                // Outputs that show a single layer go black too.
                for t in self.taps.values() {
                    self.blitter.draw_blend(ctx, &self.black_tex, Shade::Rgba, &t.rtv, full, &alpha, self.blackout);
                }
            }
            self.stash.as_texture()
        } else {
            out
        };
        // Transition Manager preview: A → B on a 2 s loop.
        if let (Some((name, [a, b])), Some(p)) = (&self.tr_preview, &self.tr_preview_rt) {
            if let Some((entry, program)) = self.lib.find_transition(name).and_then(|e| e.program.as_ref().map(|pr| (e, pr))) {
                let progress = ((time % 2.0) / 1.5).min(1.0) as f32;
                let params = FxParams { time: time as f32, beat: beat as f32, progress, values: resolve(&entry.meta, &EffectRef::new(name), beat) };
                let t = chains.fx.pass(&self.gpu, program, a, Some(b), &params);
                if unsafe { (Interface::vtable(&p.mutex).AcquireSync)(Interface::as_raw(&p.mutex), 0, 0) } == S_OK {
                    self.blitter.draw(ctx, &t, Shade::Rgba, &p.rt.rtv, [0.0, 0.0, p.rt.width as f32, p.rt.height as f32]);
                    unsafe {
                        let _ = p.mutex.ReleaseSync(0);
                    }
                }
            }
        }
        // Preview monitor: the cued clip with its effects (the composition's scratch target is free now).
        if let (true, Some(p)) = (self.cue_scene, &self.cue_rt) {
            // A scene: every cued layer blended with its layer's opacity / blend (no effects).
            if self.cue_comp.is_none() {
                self.cue_comp = Compositor::new(&self.gpu, p.rt.width, p.rt.height).ok();
            }
            if let Some(comp) = self.cue_comp.as_mut() {
                comp.begin(ctx);
                for (l, props) in self.cue.iter().zip(&self.props) {
                    let Some(a) = l.active.as_ref().filter(|_| !props.bypass) else { continue };
                    let Some(tex) = a.tex.as_ref() else { continue };
                    let rect = fit_rect(tex.width, tex.height, comp.width, comp.height, fit_index(a.clip.fit));
                    comp.layer(ctx, tex, a.shade, rect, props.blend.index(), props.opacity);
                }
                if unsafe { (Interface::vtable(&p.mutex).AcquireSync)(Interface::as_raw(&p.mutex), 0, 0) } == S_OK {
                    self.blitter.draw(ctx, &comp.output().as_texture(), Shade::Rgba, &p.rt.rtv, [0.0, 0.0, p.rt.width as f32, p.rt.height as f32]);
                    unsafe {
                        let _ = p.mutex.ReleaseSync(0);
                    }
                }
            }
        } else if let (Some(a), Some(p)) = (self.cue[0].active.as_ref(), &self.cue_rt) {
            if let Some(tex) = a.tex.as_ref() {
                let (w, h) = (self.comp.width, self.comp.height);
                let clip = Pending::Clip { tex, shade: a.shade, rect: fit_rect(tex.width, tex.height, w, h, fit_index(a.clip.fit)) };
                let pending = chains.apply_pending(&mut self.comp, "cue", &a.clip.effects, clip);
                let t = chains.materialize(&mut self.comp, pending);
                if unsafe { (Interface::vtable(&p.mutex).AcquireSync)(Interface::as_raw(&p.mutex), 0, 0) } == S_OK {
                    unsafe { ctx.ClearRenderTargetView(&p.rt.rtv, &[0.0, 0.0, 0.0, 1.0]) };
                    self.blitter.draw(ctx, &t, Shade::Rgba, &p.rt.rtv, [0.0, 0.0, p.rt.width as f32, p.rt.height as f32]);
                    unsafe {
                        let _ = p.mutex.ReleaseSync(0);
                    }
                }
            }
        }
        let used = chains.used;
        self.histories.retain(|k, _| used.contains(k));
        // Layers nobody shows any more lose their tap (an empty layer shows black).
        self.wanted_taps = self.outputs.iter().filter_map(|o| match o.config.source {
            OutputSource::Layer(n) => Some(n),
            OutputSource::Composition => None,
        }).chain(self.wanted_taps.iter().copied().filter(|n| *n < self.layers.len())).collect();
        let shown: HashSet<usize> = self.layers.iter().enumerate().filter(|(_, l)| l.active.is_some() || l.outgoing.is_some()).map(|(i, _)| i).collect();
        self.taps.retain(|n, _| shown.contains(n));
        let out = if self.outputs.iter().any(|o| o.config.test_pattern) { self.park(out) } else { out };
        self.last_out = Some(out.clone());
        let configs: Vec<OutputConfig> = self.outputs.iter().map(|o| o.config.clone()).collect();
        for (i, config) in configs.iter().enumerate() {
            if timing {
                if let Some(t) = self.timer.as_mut() {
                    t.mark(&self.gpu.ctx);
                    stages.push(format!("output {}", i + 1));
                }
            }
            let src = self.output_source(config, &out, i);
            let _ = self.outputs[i].present(&self.gpu, &self.blitter, &src, i == 0);
        }
        if timing {
            if let Some(t) = self.timer.as_mut() {
                t.mark(&self.gpu.ctx);
                stages.push("preview".into());
            }
        }
        if let Some(p) = &self.preview {
            // Never wait for the UI: skip this preview frame if it holds the texture.
            let acquired = unsafe { (Interface::vtable(&p.mutex).AcquireSync)(Interface::as_raw(&p.mutex), 0, 0) } == S_OK;
            if acquired {
                self.blitter.draw(&self.gpu.ctx, &out, Shade::Rgba, &p.rt.rtv, [0.0, 0.0, p.rt.width as f32, p.rt.height as f32]);
                unsafe {
                    let _ = p.mutex.ReleaseSync(0);
                }
            }
        }
        if timing {
            if let Some(t) = self.timer.as_mut() {
                t.end(&self.gpu.ctx);
                if let Some(ms) = t.last_ms.take() {
                    self.gpu_times.push_back(ms);
                    if self.gpu_times.len() > 60 {
                        self.gpu_times.pop_front();
                    }
                    // Stage durations from the cumulative marks, smoothed; names are this frame's
                    // (a frame measured a few frames ago had the same stages unless the setup changed).
                    let marks = std::mem::take(&mut t.last_marks);
                    if stages.len() + 1 == marks.len() {
                        if self.stage_names != stages {
                            self.stage_names = stages.clone();
                            self.gpu_stages = stages.iter().map(|n| (n.clone(), 0.0)).collect();
                        }
                        let mut prev = 0.0;
                        for (k, &m) in marks.iter().enumerate() {
                            if k > 0 {
                                let d = m - prev;
                                let s = &mut self.gpu_stages[k - 1].1;
                                *s = if *s == 0.0 { d } else { *s * 0.95 + d * 0.05 };
                            }
                            prev = m;
                        }
                    }
                }
            }
        }
        self.frames += 1;
    }

    /// Rebuilds everything that lives on the GPU after the device was removed (driver reset, TDR).
    /// Outputs keep their windows and settings, layers re-open their clips.
    fn recover(&mut self) -> Result<()> {
        let mut last = None;
        let gpu = (0..20).find_map(|_| match Gpu::new(self.kind) {
            Ok(g) => Some(g),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(250)); // the driver needs a moment after a TDR
                None
            }
        });
        let gpu = gpu.ok_or_else(|| last.unwrap_or_else(|| anyhow!("no GPU")))?;
        let (w, h) = (self.comp.width, self.comp.height);
        let comp = Compositor::new(&gpu, w, h)?;
        let blitter = Blitter::new(&gpu)?;
        let fx = FxRunner::new(&gpu, w, h)?;
        let stash = RenderTarget::new(&gpu, w, h)?;
        let clear_tex = Texture::new_color(&gpu, windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM, 1, 1)?;
        clear_tex.upload(&gpu.ctx, &[0, 0, 0, 0], 4);
        let black_tex = Texture::new_color(&gpu, windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM, 1, 1)?;
        black_tex.upload(&gpu.ctx, &[0, 0, 0, 255], 4);
        self.black_tex = black_tex;
        let lib = Library::new(&gpu, self.effect_folders.clone());
        // One swapchain per window: release the old ones before creating new ones.
        let old: Vec<(u32, isize, (u32, u32), OutputConfig)> = self.outputs.drain(..).map(|o| (o.id, o.hwnd, o.size(), o.config.clone())).collect();
        for (id, hwnd, (ow, oh), config) in old {
            match Output::new(&gpu, id, hwnd, ow, oh) {
                Ok(mut o) => {
                    o.config = config;
                    self.outputs.push(o);
                }
                Err(e) => evj_core::log::warn("engine", &format!("output {id} after GPU reset: {e:#}")),
            }
        }
        for l in &mut self.layers {
            l.restart(&gpu.device);
        }
        for l in &mut self.cue {
            l.restart(&gpu.device);
        }
        self.cue_comp = None;
        (self.comp, self.blitter, self.fx, self.stash, self.clear_tex, self.lib) = (comp, blitter, fx, stash, clear_tex, lib);
        self.timer = evj_render::GpuTimer::new(&gpu).ok();
        self.gpu_times.clear();
        self.gpu = gpu;
        self.lib_version = u64::MAX;
        self.preview = None;
        self.tr_preview = None;
        self.tr_preview_rt = None;
        self.cue_rt = None;
        self.taps.clear();
        self.histories.clear();
        self.last_out = None;
        self.device_resets += 1;
        Ok(())
    }

    /// Starts, levels and stops clip sounds; any voice no layer refers to any more fades out.
    fn update_audio(&mut self) {
        if let Some(audio) = self.audio.as_mut() {
            audio.poll();
            let mut referenced = HashSet::new();
            for (l, p) in self.layers.iter_mut().zip(&self.props) {
                let target = if p.mute || p.bypass { 0.0 } else { p.volume.clamp(0.0, 2.0) };
                drive_voices(audio, l, target, &mut self.next_voice, &mut referenced);
            }
            stop_unreferenced(audio, &referenced);
        }
        // Preview: the cue layers at unity; the Preview fader is that output's master.
        let lost = self.preview_audio.as_mut().is_some_and(|pa| {
            pa.poll();
            pa.failed()
        });
        if lost {
            let name = self.preview_audio.take().map(|a| a.device().to_string()).unwrap_or_default();
            self.preview_audio_error = Some(format!("{name} disconnected"));
            for l in &mut self.cue {
                forget_voices(l);
            }
        }
        if let Some(pa) = self.preview_audio.as_mut() {
            let mut referenced = HashSet::new();
            if self.cue_rt.is_some() {
                for l in &mut self.cue {
                    drive_voices(pa, l, 1.0, &mut self.next_voice, &mut referenced);
                }
            }
            stop_unreferenced(pa, &referenced);
        }
    }

    /// Copies `out` into `stash` so effect passes can reuse the scratch targets it may live in.
    fn park(&self, out: Texture) -> Texture {
        if out.tex == self.stash.tex {
            return out;
        }
        unsafe { self.gpu.ctx.CopyResource(&self.stash.tex, &out.tex) };
        self.stash.as_texture()
    }

    /// The picture an output shows: test pattern, a layer tap, or the composition.
    fn output_source(&mut self, config: &OutputConfig, out: &Texture, index: usize) -> Texture {
        if config.test_pattern {
            if let Some((entry, program)) = self.lib.find("Test Pattern").and_then(|e| e.program.as_ref().map(|p| (e, p))) {
                let mut e = EffectRef::new("Test Pattern");
                e.param_mut("number", 1.0).value = (index + 1) as f64;
                let params = FxParams { time: self.time as f32, beat: self.tempo.beat as f32, progress: 0.0, values: resolve(&entry.meta, &e, 0.0) };
                return self.fx.pass(&self.gpu, program, out, None, &params);
            }
        }
        match config.source {
            OutputSource::Composition => out.clone(),
            OutputSource::Layer(n) => self.taps.get(&n).map(RenderTarget::as_texture).unwrap_or_else(|| self.clear_tex.clone()),
        }
    }

    fn snapshot(&self) -> Snapshot {
        let layers = self
            .layers
            .iter()
            .map(|l| {
                let a = l.active.as_ref();
                let live: Vec<&evj_audio::VoiceHandle> = a.into_iter().flat_map(|a| [a.voice.as_ref(), a.attached_voice.as_ref()]).flatten().filter(|v| !v.finished()).collect();
                let voices = live.len() as u8;
                let peaks = live.iter().fold([0.0f32; 2], |m, v| {
                    let p = v.peaks();
                    [m[0].max(p[0]), m[1].max(p[1])]
                });
                LayerState {
                    clip_name: a.map(|a| a.clip.name.clone()),
                    pos: a.map_or(0.0, |a| a.position()),
                    duration: a.map_or(0.0, |a| a.duration()),
                    kind: l.kind(),
                    random_access: a.is_some_and(|a| a.player.info.random_access),
                    has_frame: a.is_some_and(|a| a.tex.is_some()),
                    finished: a.is_some_and(|a| a.finished()),
                    loading: l.loading(),
                    error: l.error.clone(),
                    transition: l.outgoing.as_ref().map(|(_, r)| r.progress()),
                    audio: voices > 0,
                    audio_peak: peaks[0].max(peaks[1]),
                    audio_peaks: peaks,
                    voices,
                    remaining: a.and_then(|a| a.remaining(self.tempo.bpm)),
                    looping: a.is_some_and(|a| a.clip.mode == PlayMode::Loop),
                    slide: l.deck.as_ref().map(|(d, p, _)| (p.slide, p.step, d.slides.len())),
                    start: a.map_or(0.0, |a| a.range().0),
                    end: a.map_or(0.0, |a| a.range().1),
                    clip_path: l.deck.as_ref().map(|(_, _, t)| t.path.clone()).or_else(|| a.map(|a| a.clip.path.clone())),
                    queue_taken: l.queue_taken,
                    queue_failed: l.queue_failed,
                }
            })
            .collect();
        Snapshot {
            adapter: self.gpu.adapter_name.clone(),
            layers,
            frames: self.frames,
            fps: self.stats.recent_fps(),
            p99_ms: self.stats.recent_percentile(99.0),
            dropped: self.stats.dropped,
            outputs: self.outputs.len(),
            resolution: (self.comp.width, self.comp.height),
            preview: self.preview.is_some(),
            bpm: self.tempo.bpm,
            beat: self.tempo.beat,
            effects: self.infos.clone(),
            transition_preview: self.tr_preview.as_ref().map(|(n, _)| n.clone()),
            cue: self.cue.iter().filter_map(|l| l.active.as_ref()).find(|a| a.tex.is_some()).map(|a| a.clip.name.clone()),
            cue_layers: self.cue.iter().filter(|l| l.active.is_some()).count(),
            // A scene's transport follows its longest clip.
            cue_state: self.cue.iter().filter_map(|l| l.active.as_ref()).filter(|a| a.tex.is_some()).max_by(|x, y| x.duration().total_cmp(&y.duration())).map(|a| CueState {
                name: a.clip.name.clone(),
                pos: a.position(),
                duration: a.duration(),
                remaining: a.remaining(self.tempo.bpm),
                paused: a.paused,
                start: a.range().0,
                end: a.range().1,
            }),
            master_peaks: self.audio.as_ref().map_or([0.0; 2], |a| a.master_peaks()),
            master_clip: self.audio.as_ref().is_some_and(|a| a.master_clip()),
            blackout: self.blackout,
            device_resets: self.device_resets,
            frame_times: self.stats.tail(120),
            gpu_memory_mb: self.gpu_memory_mb,
            gpu_stages: self.gpu_stages.clone(),
            gpu_ms: if self.gpu_times.is_empty() { 0.0 } else { self.gpu_times.iter().sum::<f32>() / self.gpu_times.len() as f32 },
            audio_device: self.audio.as_ref().map(|a| a.device().to_string()),
            preview_audio_device: self.preview_audio.as_ref().map(|a| a.device().to_string()),
            preview_peaks: self.preview_audio.as_ref().map_or([0.0; 2], |a| a.master_peaks()),
            preview_audio_error: self.preview_audio_error.clone(),
            master_peak: self.audio.as_ref().map_or(0.0, |a| a.master_peak()),
            audio_underruns: self.audio.as_ref().map_or(0, |a| a.underruns()),
        }
    }
}
