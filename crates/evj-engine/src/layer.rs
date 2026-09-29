//! Per-layer playback: async clip opening, playhead, frame selection and GPU upload.
use anyhow::{Context, Result};
use evj_core::model::{Clip, PlayMode};
use evj_core::playhead::{Playhead, effective_speed, frame_index};
use evj_core::transition::{TransitionPreset, TransitionRun};
use evj_media::hap::HapFormat;
use evj_audio::VoiceHandle;
use evj_core::slides::{SlideDeck, SlidePos};
use evj_media::{ClipPlayer, DecoderKind, Frame, FrameData, Msg, StartAt, open_clip_from};
use evj_render::{Gpu, Shade, Texture};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use windows::Win32::Graphics::Direct3D11::{D3D11_BOX, D3D11_TEXTURE2D_DESC, ID3D11Device};
use windows::Win32::Graphics::Dxgi::Common::*;

/// An opened clip and the length of its attached audio (0 = none / unknown).
type Opened = (ClipPlayer, f64);

/// Random-access frames kept around for look-ahead.
const CACHE: usize = 8;

struct SendDevice(ID3D11Device);
// SAFETY: the engine device is multithread-protected.
unsafe impl Send for SendDevice {}

/// A clip that is playing (or about to).
pub struct Active {
    pub clip: Clip,
    pub player: ClipPlayer,
    head: Playhead,
    seq_time: f64,
    cache: Vec<Frame>,
    pending: Option<Frame>,
    pub tex: Option<Texture>,
    pub shade: Shade,
    shown: Option<u64>,
    ended: bool,
    /// The clip's sound, when it has one and plays at normal speed.
    pub voice: Option<VoiceHandle>,
    /// Clip time (s) the voice started at; its position is added to this.
    pub audio_start: f64,
    pub audio_rate: u32,
    pub audio_started: bool,
    pub audio_stopping: bool,
    pub gain_sent: f32,
    /// Slides: hold the picture at this clip time (a click boundary).
    pub pause_at: Option<f64>,
    /// Held (Preview transport): neither advances nor uploads.
    pub paused: bool,
    /// Reached the out point of a looping sequential clip: re-open at the in point.
    wrap: bool,
    /// The sound must start again at the current position (seek, A–B loop).
    pub audio_restart: bool,
    /// Attached audio: its length (probed when opening), voice and state.
    pub attached_secs: f64,
    pub attached_voice: Option<VoiceHandle>,
    pub attached_start: f64,
    pub attached_started: bool,
    pub attached_stopping: bool,
    pub attached_gain: f32,
    pub attached_error_shown: bool,
}

impl Active {
    fn new(clip: Clip, (player, attached_secs): Opened, start: StartAt, pause_at: Option<f64>) -> Active {
        let start = start.secs(player.info.duration);
        let mut head = Playhead::start(&clip, player.info.duration);
        if start > 0.0 {
            head.pos = start;
        }
        Active {
            clip,
            player,
            head,
            seq_time: start,
            cache: Vec::new(),
            pending: None,
            tex: None,
            shade: Shade::Rgba,
            shown: None,
            ended: false,
            voice: None,
            audio_start: 0.0,
            audio_rate: 48_000,
            audio_started: false,
            audio_stopping: false,
            gain_sent: -1.0,
            pause_at,
            paused: false,
            wrap: false,
            audio_restart: false,
            attached_secs,
            attached_voice: None,
            attached_start: 0.0,
            attached_started: false,
            attached_stopping: false,
            attached_gain: -1.0,
            attached_error_shown: false,
        }
    }

    /// A still whose length comes from its attached audio.
    pub fn timed_by_attached(&self) -> bool {
        self.player.info.kind == DecoderKind::Image && self.attached_secs > 0.0 && self.clip.attached.is_some()
    }

    /// Clip length (seconds); an image lasts its own duration, or as long as its attached audio.
    pub fn duration(&self) -> f64 {
        if self.player.info.kind == DecoderKind::Image { self.range().1 } else { self.player.info.duration }
    }

    /// Attached audio clock (seconds since it started) while it plays.
    fn attached_time(&self) -> Option<f64> {
        let v = self.attached_voice.as_ref().filter(|v| !v.finished() && v.position_frames() > 0)?;
        Some(self.attached_start + v.position_secs(self.audio_rate))
    }

    /// The clip-time range that plays: (start, end) seconds. Images: (0, their duration).
    pub fn range(&self) -> (f64, f64) {
        let d = self.player.info.duration;
        if self.timed_by_attached() {
            return (0.0, self.attached_secs);
        }
        if self.player.info.kind == DecoderKind::Image {
            return (0.0, self.clip.image_secs());
        }
        if d <= 0.0 { (0.0, 0.0) } else { evj_core::playhead::in_out(&self.clip, d) }
    }

    /// A start or end point inside the file: the decoder's own loop (whole file) does not
    /// honour it, so the layer wraps / stops the clip itself.
    pub(crate) fn trimmed(&self) -> bool {
        let (a, b) = self.range();
        let d = self.player.info.duration;
        self.player.info.kind != DecoderKind::Image && d > 0.0 && (a > 1e-3 || b < d - 1e-3)
    }

    /// Sound only at normal forward speed: reverse / ping-pong / BPM-stretched clips stay silent.
    pub fn audible(&self) -> bool {
        // Replace: the attached audio takes the clip's own sound's place.
        let replaced = self.clip.attached.as_ref().is_some_and(|x| !x.mix && !x.path.as_os_str().is_empty());
        !replaced && self.player.info.has_audio && self.clip.audio && (self.clip.speed - 1.0).abs() < 1e-6 && self.clip.bpm_beats.is_none() && self.clip.mode != PlayMode::PingPong
    }

    /// The whole file loops (not an in/out range), so the audio can loop with it.
    pub fn loops_whole_file(&self) -> bool {
        self.clip.mode == PlayMode::Loop && self.clip.in_point <= 0.0 && self.clip.out_point >= 1.0
    }

    /// Audio clock (clip seconds) while the voice plays.
    fn audio_time(&self) -> Option<f64> {
        let v = self.voice.as_ref().filter(|v| !v.finished() && v.position_frames() > 0)?;
        Some(self.audio_start + v.position_secs(self.audio_rate))
    }

    /// Seconds into the clip, for display.
    pub fn position(&self) -> f64 {
        let info = &self.player.info;
        if info.kind == DecoderKind::Image {
            self.seq_time
        } else if info.random_access {
            self.head.pos
        } else if self.clip.mode != PlayMode::Once && info.duration > 0.0 && !self.trimmed() {
            self.seq_time % info.duration
        } else {
            self.seq_time.min(info.duration.max(self.seq_time))
        }
    }

    pub fn finished(&self) -> bool {
        self.head.finished || self.ended
    }

    /// Where playback is now (seconds), for starting the sound in step with the picture.
    pub fn clip_time(&self) -> f64 {
        let info = &self.player.info;
        if info.random_access && info.kind != DecoderKind::Image { self.head.pos } else { self.seq_time }
    }

    /// Seconds until the clip ends (out point) at its current speed; None for unknown length.
    pub fn remaining(&self, bpm: f64) -> Option<f64> {
        let info = &self.player.info;
        if self.timed_by_attached() {
            return Some((self.attached_secs - self.seq_time).max(0.0));
        }
        if info.kind == DecoderKind::Image {
            return Some((self.clip.image_secs() - self.seq_time).max(0.0));
        }
        if info.duration <= 0.0 {
            return None;
        }
        let d = info.duration;
        let (a, b) = self.range();
        let v = if info.random_access { effective_speed(&self.clip, d, bpm) * self.head.dir } else { self.clip.speed.max(0.0) };
        Some(evj_core::playhead::remaining(self.position(), a, b, v))
    }

    /// Back to the start (in point); random-access sources only (others are re-opened).
    pub fn rewind(&mut self) {
        self.head = Playhead::start(&self.clip, self.player.info.duration);
        self.shown = None;
    }

    fn update(&mut self, gpu: &Gpu, dt: f64, bpm: f64) -> Result<()> {
        // Paused: hold the picture — but after a rewind (nothing shown yet) fetch the new frame.
        if self.paused && self.shown.is_some() {
            return Ok(());
        }
        let dt = if self.paused { 0.0 } else { dt };
        let info = self.player.info.clone();
        // Images run a clock over their duration: Once stops at the end (the picture stays), else it wraps.
        if self.timed_by_attached() {
            let len = self.attached_secs;
            if let Some(t) = self.attached_time() {
                self.seq_time = if self.clip.mode == PlayMode::Loop { t.rem_euclid(len) } else { t.min(len) };
            }
            if self.clip.mode != PlayMode::Loop && self.attached_voice.as_ref().is_some_and(|v| v.finished()) {
                self.seq_time = len;
                self.ended = true;
            }
        } else if info.kind == DecoderKind::Image {
            let len = self.clip.image_secs().max(1e-3);
            if self.clip.mode == PlayMode::Once {
                self.seq_time = (self.seq_time + dt).min(len);
                self.ended |= self.seq_time >= len;
            } else {
                self.seq_time = (self.seq_time + dt) % len;
            }
        }
        let audio_t = self.audio_time();
        if info.kind == DecoderKind::Audio {
            if let Some(t) = audio_t {
                self.seq_time = t;
            }
            let (a, b) = self.range();
            if self.trimmed() && self.seq_time >= b {
                if self.clip.mode == PlayMode::Once {
                    self.seq_time = b;
                    self.ended = true;
                    return Ok(());
                }
                self.seq_time = a;
                self.audio_restart = true;
            }
            self.ended = self.voice.as_ref().is_some_and(|v| v.finished()) && self.clip.mode == PlayMode::Once;
            return Ok(());
        }
        if info.random_access {
            match audio_t {
                // Picture follows the sound (A/V sync).
                Some(t) if self.loops_whole_file() && info.duration > 0.0 => self.head.pos = t % info.duration,
                Some(t) => {
                    let (a, b) = self.range();
                    if t >= b && self.trimmed() {
                        if self.clip.mode == PlayMode::Loop {
                            self.head.pos = a;
                            self.audio_restart = true;
                        } else {
                            self.head.pos = b;
                            self.ended = true;
                        }
                    } else {
                        self.head.pos = t.min(info.duration);
                    }
                }
                None => self.head.advance(dt, &self.clip, info.duration, bpm),
            }
            let n = info.frame_count.unwrap_or(1);
            let want = frame_index(self.head.pos, info.fps, n);
            let forward = effective_speed(&self.clip, info.duration, bpm) * self.head.dir >= 0.0;
            self.player.request(want, forward);
            loop {
                match self.player.frames.try_recv() {
                    Ok(Ok(Msg::Frame(f))) => {
                        self.cache.retain(|c| c.index != f.index);
                        self.cache.push(f);
                        if self.cache.len() > CACHE {
                            self.cache.remove(0);
                        }
                    }
                    Ok(Ok(Msg::End)) | Err(TryRecvError::Empty) => break,
                    Ok(Err(e)) => return Err(e),
                    Err(TryRecvError::Disconnected) => anyhow::bail!("decoder stopped"),
                }
            }
            if self.shown != Some(want) {
                if let Some(i) = self.cache.iter().position(|f| f.index == want) {
                    let f = self.cache.swap_remove(i);
                    self.upload(gpu, f)?;
                    self.shown = Some(want);
                }
            }
        } else {
            // Sequential sources only play forward; with sound the audio clock leads.
            match audio_t {
                Some(t) if !self.ended => self.seq_time = t,
                None if !self.ended => self.seq_time += dt * self.clip.speed.max(0.0),
                _ => {}
            }
            if let Some(p) = self.pause_at {
                self.seq_time = self.seq_time.min(p);
            }
            if self.trimmed() && self.seq_time >= self.range().1 {
                self.seq_time = self.range().1;
                if self.clip.mode == PlayMode::Once {
                    self.ended = true;
                } else {
                    self.wrap = true;
                }
            }
            let due = self.seq_time + 0.5 / info.fps;
            let mut newest = None;
            loop {
                let f = match self.pending.take() {
                    Some(f) => f,
                    None => match self.player.frames.try_recv() {
                        Ok(Ok(Msg::Frame(f))) => f,
                        Ok(Ok(Msg::End)) => {
                            self.ended = true;
                            break;
                        }
                        Ok(Err(e)) => return Err(e),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => anyhow::bail!("decoder stopped"),
                    },
                };
                // Always show the first frame, even if the clock has not reached it.
                if f.pts <= due || self.shown.is_none() && newest.is_none() {
                    newest = Some(f);
                } else {
                    self.pending = Some(f);
                    break;
                }
            }
            if let Some(f) = newest {
                self.shown = Some(f.index);
                self.upload(gpu, f)?;
            }
        }
        Ok(())
    }

    fn upload(&mut self, gpu: &Gpu, f: Frame) -> Result<()> {
        let (w, h, bt709) = (self.player.info.width, self.player.info.height, self.player.info.bt709);
        let fits = |t: &Option<Texture>, w: u32, h: u32| t.as_ref().is_some_and(|t| t.width == w && t.height == h);
        match f.data {
            FrameData::Bc { format, data } => {
                let (dxgi, shade) = match format {
                    HapFormat::Bc1 => (DXGI_FORMAT_BC1_UNORM, Shade::Rgba),
                    HapFormat::Bc3 => (DXGI_FORMAT_BC3_UNORM, Shade::Rgba),
                    HapFormat::YCoCgBc3 => (DXGI_FORMAT_BC3_UNORM, Shade::YCoCg),
                };
                if !fits(&self.tex, w, h) {
                    self.tex = Some(Texture::new_bc_dynamic(gpu, dxgi, w, h)?);
                }
                self.shade = shade;
                let pitch = w.div_ceil(4) as usize * format.block_bytes();
                self.tex.as_ref().context("texture")?.upload(&gpu.ctx, &data, pitch as u32);
            }
            FrameData::Gpu { texture, subresource, sample: _sample } => {
                let mut desc = D3D11_TEXTURE2D_DESC::default();
                unsafe { texture.GetDesc(&mut desc) };
                let nv12 = desc.Format == DXGI_FORMAT_NV12;
                // NV12 planes are subsampled 2x2: keep dimensions even.
                let (w, h) = if nv12 { (w & !1, h & !1) } else { (w, h) };
                if !fits(&self.tex, w, h) {
                    self.tex = Some(if nv12 { Texture::new_nv12(gpu, w, h)? } else { Texture::new_color(gpu, desc.Format, w, h)? });
                }
                self.shade = if nv12 { Shade::Nv12 { bt709 } } else { Shade::Rgba };
                let tex = self.tex.as_ref().context("texture")?;
                // Decoder surfaces are padded (e.g. 1088 rows); copy only the visible part.
                let bx = D3D11_BOX { left: 0, top: 0, front: 0, right: w, bottom: h, back: 1 };
                unsafe { gpu.ctx.CopySubresourceRegion(&tex.tex, 0, 0, 0, 0, &texture, subresource, Some(&bx)) };
            }
            FrameData::Bgra { data } => {
                if !fits(&self.tex, w, h) {
                    self.tex = Some(Texture::new_color(gpu, DXGI_FORMAT_B8G8R8A8_UNORM, w, h)?);
                }
                self.shade = Shade::Rgba;
                self.tex.as_ref().context("texture")?.upload(&gpu.ctx, &data, w * 4);
            }
        }
        Ok(())
    }
}

/// One composition layer: the clip on screen, the one being opened / warmed up, and the one
/// transitioning out.
#[derive(Default)]
pub struct LayerRt {
    pub active: Option<Active>,
    incoming: Option<(Active, Option<TransitionPreset>)>,
    opening: Option<(Clip, Option<TransitionPreset>, (StartAt, Option<f64>), Receiver<Result<Opened>>)>,
    /// A presentation: the deck, where we are, and the deck clip's settings (fit, effects).
    pub deck: Option<(SlideDeck, SlidePos, Clip)>,
    /// Previous clip, still playing while the transition runs.
    pub outgoing: Option<(Active, TransitionRun)>,
    pub error: Option<String>,
    /// The next slide's media, opened ahead so a click shows it at once: (path, start, player).
    preload: Option<(std::path::PathBuf, StartAt, Receiver<Result<Opened>>)>,
    /// The next clip to swap in starts paused (Preview stop / rewind of a sequential source).
    pub start_paused: bool,
    /// Clip that follows when the current one finishes (sequences), opened ahead.
    queued: Option<(Clip, Receiver<Result<Opened>>)>,
    /// How many queued clips were swapped in (the app follows its sequences by this).
    pub queue_taken: u64,
    /// How many queued clips failed to open (the current clip stays on).
    pub queue_failed: u64,
    /// The clip being opened came from the queue.
    from_queue: bool,
}

/// Opens `path` on a worker thread (decoders can take a while to start).
/// The attached audio probe needs its length; it runs on the same worker.
fn spawn_open(device: &ID3D11Device, clip: &Clip, start: StartAt) -> std::io::Result<Receiver<Result<Opened>>> {
    let (tx, rx) = channel();
    let dev = SendDevice(device.clone());
    let (path, looping) = (clip.path.clone(), clip.mode != PlayMode::Once);
    let attached = clip.attached.as_ref().map(|a| a.path.clone()).filter(|p| !p.as_os_str().is_empty());
    std::thread::Builder::new().name("evj-open".into()).spawn(move || {
        let dev = dev;
        let r = open_clip_from(&path, Some(dev.0), looping, start).map(|p| {
            let secs = attached.map_or(0.0, |a| {
                let _ = evj_media::mf::mf_init_thread();
                evj_media::audio::AudioDecoder::open(&a, 8000).map_or(0.0, |d| d.duration)
            });
            (p, secs)
        });
        let _ = tx.send(r);
    })?;
    Ok(rx)
}

/// The clip that shows slide step `pos` of a deck: (clip, start, pause_at).
fn slide_clip(deck: &SlideDeck, template: &Clip, pos: SlidePos, at_end: bool) -> (Clip, StartAt, Option<f64>) {
    let slide = &deck.slides[pos.slide];
    let (_, to) = deck.segment(pos);
    let mut clip = Clip { name: String::new(), transition: None, ..template.clone() };
    let (start, pause) = match &slide.video {
        Some(v) => {
            clip.path = v.clone();
            clip.mode = PlayMode::Once;
            // Sound only for single-step slides (embedded videos); a paused click would cut it.
            clip.audio = slide.steps.len() <= 1;
            (if at_end { (to - 0.05).max(0.0) } else { 0.0 }, Some(to))
        }
        None => {
            clip.path = slide.image.clone();
            (0.0, None)
        }
    };
    clip.name = clip.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    (clip, StartAt::Secs(start), pause)
}

impl LayerRt {
    pub fn trigger(&mut self, device: &ID3D11Device, clip: Clip, transition: Option<TransitionPreset>) {
        self.deck = None;
        self.preload = None;
        self.queued = None;
        let start = StartAt::Fraction(clip.in_point);
        self.open(device, clip, transition, start, None);
    }

    fn open(&mut self, device: &ID3D11Device, clip: Clip, transition: Option<TransitionPreset>, start: StartAt, pause_at: Option<f64>) {
        self.incoming = None;
        self.from_queue = false;
        // Opened ahead (next slide)? Then it is ready, or nearly.
        if let Some((_, _, rx)) = self.preload.take().filter(|(p, s, _)| *p == clip.path && *s == start) {
            self.opening = Some((clip, transition, (start, pause_at), rx));
            return;
        }
        match spawn_open(device, &clip, start) {
            Ok(rx) => self.opening = Some((clip, transition, (start, pause_at), rx)),
            Err(e) => self.error = Some(format!("cannot start decoder: {e}")),
        }
    }

    /// Empties the layer, fading the current clip out when a transition is given.
    pub fn clear(&mut self, transition: Option<TransitionPreset>, bpm: f64) {
        let fade = transition.filter(|t| !t.is_cut()).zip(self.active.take().filter(|a| a.tex.is_some()));
        *self = LayerRt { queue_taken: self.queue_taken, queue_failed: self.queue_failed, ..LayerRt::default() };
        if let Some((t, a)) = fade {
            self.outgoing = Some((a, TransitionRun::new(&t, bpm)));
        }
    }

    /// Starts a presentation (`deck.json`) at its first slide.
    pub fn open_deck(&mut self, device: &ID3D11Device, clip: Clip, transition: Option<TransitionPreset>) {
        match SlideDeck::load(&clip.path) {
            Ok(deck) if !deck.slides.is_empty() => {
                self.queued = None;
                self.deck = Some((deck, SlidePos::default(), clip));
                self.show_slide(device, SlidePos::default(), transition, false);
            }
            Ok(_) => self.error = Some("the presentation has no slides".into()),
            Err(e) => self.error = Some(format!("{e:#}")),
        }
    }

    /// Opens the media of `pos`: the slide video paused at the step's click, or the slide image.
    /// `at_end`: show the step already finished (going backwards).
    fn show_slide(&mut self, device: &ID3D11Device, pos: SlidePos, transition: Option<TransitionPreset>, at_end: bool) {
        let Some((deck, p, template)) = self.deck.as_mut() else { return };
        *p = pos;
        let (clip, start, pause) = slide_clip(deck, template, pos, at_end);
        // The slide after this one, opened now so the next click is instant.
        let ahead = deck.next(SlidePos { slide: pos.slide, step: deck.slides[pos.slide].steps.len().max(1) - 1 }).map(|n| slide_clip(deck, template, n, false));
        self.open(device, clip, transition, start, pause);
        self.preload = ahead.and_then(|(c, s, _)| spawn_open(device, &c, s).ok().map(|rx| (c.path, s, rx)));
    }

    /// Next click: continue the slide's animation, or go to the next slide.
    pub fn slide_next(&mut self, device: &ID3D11Device, transition: Option<TransitionPreset>) {
        let Some((deck, pos, _)) = self.deck.as_ref() else { return };
        let Some(next) = deck.next(*pos) else { return };
        if next.slide == pos.slide {
            let pause = deck.segment(next).1;
            if let Some((_, p, _)) = self.deck.as_mut() {
                *p = next;
            }
            for a in [self.active.as_mut(), self.incoming.as_mut().map(|(a, _)| a)].into_iter().flatten() {
                a.pause_at = Some(pause);
            }
            if let Some((_, _, (_, p), _)) = self.opening.as_mut() {
                *p = Some(pause);
            }
        } else {
            self.show_slide(device, next, transition, false);
        }
    }

    pub fn slide_prev(&mut self, device: &ID3D11Device, transition: Option<TransitionPreset>) {
        let Some((deck, pos, _)) = self.deck.as_ref() else { return };
        let Some(prev) = deck.prev(*pos) else { return };
        let same = prev.slide == pos.slide;
        self.show_slide(device, prev, if same { None } else { transition }, true);
    }

    pub fn slide_goto(&mut self, device: &ID3D11Device, slide: usize, transition: Option<TransitionPreset>) {
        if self.deck.as_ref().is_some_and(|(d, _, _)| slide < d.slides.len()) {
            self.show_slide(device, SlidePos { slide, step: 0 }, transition, false);
        }
    }

    /// Opens `clip` now and swaps it in when the playing clip finishes (None: forget it).
    pub fn queue(&mut self, device: &ID3D11Device, clip: Option<Clip>) {
        self.queued = None;
        let Some(c) = clip else { return };
        match spawn_open(device, &c, StartAt::Fraction(c.in_point)) {
            Ok(rx) => self.queued = Some((c, rx)),
            Err(e) => self.error = Some(format!("cannot start decoder: {e}")),
        }
    }

    /// Moves the playing clip to `secs` (clamped to its start–end). Presentations ignore it.
    pub fn seek(&mut self, device: &ID3D11Device, secs: f64) {
        if self.deck.is_some() {
            return;
        }
        let Some(a) = self.active.as_mut() else { return };
        let (lo, hi) = a.range();
        // Just short of the end: a looping clip at its very end is back at its start.
        let t = secs.clamp(lo, (hi - 1e-3).max(lo));
        let kind = a.player.info.kind;
        if kind == DecoderKind::Image || kind == DecoderKind::Audio {
            a.seq_time = t;
            a.ended = false;
            a.audio_restart = true;
        } else if a.player.info.random_access {
            a.head.pos = t;
            a.head.finished = false;
            a.ended = false;
            a.shown = None; // fetch the frame even when paused
            a.audio_restart = true;
        } else {
            let (clip, paused) = (a.clip.clone(), a.paused);
            self.start_paused = paused;
            self.open(device, clip, None, StartAt::Secs(t), None);
        }
    }

    /// Re-opens whatever the layer shows on a new device (after a GPU reset).
    pub fn restart(&mut self, device: &ID3D11Device) {
        let clip = self.opening.as_ref().map(|(c, _, _, _)| c.clone()).or_else(|| self.incoming.as_ref().map(|(a, _)| a.clip.clone())).or_else(|| self.active.as_ref().map(|a| a.clip.clone()));
        let deck = self.deck.take();
        *self = LayerRt::default();
        match deck {
            Some((d, pos, template)) => {
                self.deck = Some((d, pos, template));
                self.show_slide(device, pos, None, pos.step > 0);
            }
            None => {
                if let Some(c) = clip {
                    self.trigger(device, c, None);
                }
            }
        }
    }

    /// Settings change for the playing clip (same file).
    pub fn update_clip(&mut self, clip: Clip) {
        if let Some(a) = self.active.as_mut().filter(|a| a.clip.path == clip.path) {
            a.clip = clip;
        }
    }

    pub fn update(&mut self, gpu: &Gpu, dt: f64, bpm: f64) {
        // The playing clip is done: the queued one (already opening) takes over.
        let done = self.active.as_ref().is_some_and(|a| a.finished()) && self.opening.is_none() && self.incoming.is_none();
        if done {
            if let Some((clip, rx)) = self.queued.take() {
                let start = StartAt::Fraction(clip.in_point);
                self.opening = Some((clip, None, (start, None), rx));
                self.from_queue = true;
            }
        }
        if let Some((clip, transition, (start, pause), rx)) = &self.opening {
            match rx.try_recv() {
                Ok(Ok(player)) => {
                    self.incoming = Some((Active::new(clip.clone(), player, *start, *pause), transition.clone()));
                    self.opening = None;
                    self.error = None;
                    if std::mem::take(&mut self.from_queue) {
                        self.queue_taken += 1;
                    }
                }
                Ok(Err(e)) => {
                    self.error = Some(format!("{e:#}"));
                    self.opening = None;
                    if std::mem::take(&mut self.from_queue) {
                        self.queue_failed += 1;
                    }
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    self.error = Some("decoder thread died while opening".into());
                    self.opening = None;
                }
            }
        }
        let outgoing = self.outgoing.as_mut().map(|(a, _)| a);
        for slot in [self.incoming.as_mut().map(|(a, _)| a), self.active.as_mut(), outgoing].into_iter().flatten() {
            if let Err(e) = slot.update(gpu, dt, bpm) {
                self.error = Some(format!("{}: {e:#}", slot.clip.name));
                slot.tex = None; // shows nothing; dropped below
            }
        }
        if self.active.as_ref().is_some_and(|a| a.tex.is_none() && self.error.is_some()) {
            self.active = None;
        }
        // A looping sequential clip reached its out point: back to the in point (keeps paused).
        let wrap = self.opening.is_none() && self.incoming.is_none() && self.active.as_ref().is_some_and(|a| a.wrap);
        if let Some(a) = self.active.as_mut().filter(|_| wrap) {
            a.wrap = false;
            let (clip, paused) = (a.clip.clone(), a.paused);
            let start = StartAt::Fraction(clip.in_point);
            self.start_paused = paused;
            self.open(&gpu.device, clip, None, start, None);
        }
        if let Some((_, run)) = self.outgoing.as_mut() {
            run.advance(dt);
            if run.done() {
                self.outgoing = None;
            }
        }
        // Swap in the new clip once it has something to show — no black flash on trigger.
        if self.incoming.as_ref().is_some_and(|(a, _)| a.tex.is_some() || a.player.info.kind == DecoderKind::Audio) {
            if let Some((mut new, transition)) = self.incoming.take() {
                new.paused = std::mem::take(&mut self.start_paused);
                // A trimmed looping sequential clip: open its start now, the wrap takes it.
                if !new.player.info.random_access && new.clip.mode == PlayMode::Loop && new.trimmed() && self.deck.is_none() {
                    let start = StartAt::Fraction(new.clip.in_point);
                    self.preload = spawn_open(&gpu.device, &new.clip, start).ok().map(|rx| (new.clip.path.clone(), start, rx));
                }
                let old = self.active.replace(new);
                self.outgoing = transition
                    .filter(|t| !t.is_cut())
                    .zip(old.filter(|a| a.tex.is_some()))
                    .map(|(t, a)| (a, TransitionRun::new(&t, bpm)));
            }
        }
    }

    pub fn loading(&self) -> bool {
        self.opening.is_some() || self.incoming.is_some()
    }

    pub fn kind(&self) -> Option<DecoderKind> {
        self.active.as_ref().map(|a| a.player.info.kind)
    }
}
