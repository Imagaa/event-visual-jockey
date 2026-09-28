//! One decoder thread per playing clip, frames handed over through a small bounded channel.
//!
//! Random-access sources (HAP, images) decode what the engine asks for via [`ClipPlayer::request`]
//! plus a short look-ahead in the playing direction. Sequential sources (Media Foundation, FFmpeg)
//! decode forward on their own, looping or ending with [`Msg::End`].
use crate::hap::{HapFormat, HapReader, is_hap};
use crate::image::{is_image, load_image};
use crate::mf::{MfVideo, VideoFrame, mf_init_thread};
use crate::mov::read_video_track;
use anyhow::{Result, anyhow, bail};
use std::collections::VecDeque;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::JoinHandle;
use std::time::Duration;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11Texture2D};
use windows::Win32::Media::MediaFoundation::IMFSample;

/// Largest still-image side; bigger photos are scaled down on load.
const MAX_IMAGE_SIDE: u32 = 4096;
/// Frames decoded beyond the requested one (random access).
const LOOK_AHEAD: u64 = 2;
const NO_REQUEST: u64 = u64::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderKind {
    Hap,
    Image,
    MediaFoundation,
    Ffmpeg,
    /// Sound only (MP3, WAV, ...): nothing to show.
    Audio,
}

#[derive(Debug, Clone)]
pub struct ClipInfo {
    pub kind: DecoderKind,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub frame_count: Option<u64>,
    /// YUV matrix of NV12 GPU frames (Media Foundation only).
    pub bt709: bool,
    /// Seconds; 0 for stills.
    pub duration: f64,
    /// Frames can be requested in any order (HAP, images).
    pub random_access: bool,
    /// The file has an audio track (played by the audio engine, not here).
    pub has_audio: bool,
}

pub enum FrameData {
    Bc { format: HapFormat, data: Vec<u8> },
    Gpu { texture: ID3D11Texture2D, subresource: u32, sample: IMFSample },
    Bgra { data: Vec<u8> },
}

pub struct Frame {
    pub index: u64,
    pub pts: f64,
    pub data: FrameData,
}

pub enum Msg {
    Frame(Frame),
    /// A non-looping sequential clip reached its end.
    End,
}

// SAFETY: D3D11 (multithread-protected device) and Media Foundation objects are free-threaded.
unsafe impl Send for Msg {}
struct SendDevice(Option<ID3D11Device>);
unsafe impl Send for SendDevice {}

struct Request {
    index: AtomicU64,
    backward: AtomicBool,
}

pub struct ClipPlayer {
    pub info: ClipInfo,
    pub frames: Receiver<Result<Msg>>,
    request: Arc<Request>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ClipPlayer {
    /// Random-access sources: decode `index` next, then look ahead in the playing direction.
    pub fn request(&self, index: u64, forward: bool) {
        self.request.backward.store(!forward, Ordering::Relaxed);
        self.request.index.store(index, Ordering::Relaxed);
    }
}

impl Drop for ClipPlayer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        while self.frames.try_recv().is_ok() {} // unblock a producer waiting on a full channel
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

enum Source {
    Hap(HapReader),
    Image(Option<(u32, u32, Vec<u8>)>),
    Mf(MfVideo),
    #[cfg(feature = "ffmpeg")]
    Ff(crate::ff::FfVideo),
    AudioOnly,
}

fn open_source(path: &Path, device: Option<&ID3D11Device>) -> Result<(Source, ClipInfo)> {
    if is_image(path) {
        let (w, h, px) = load_image(path, MAX_IMAGE_SIDE)?;
        let info = ClipInfo {
            kind: DecoderKind::Image,
            width: w,
            height: h,
            fps: 1.0,
            frame_count: Some(1),
            bt709: false,
            duration: 0.0,
            random_access: true,
            has_audio: false,
        };
        return Ok((Source::Image(Some((w, h, px))), info));
    }
    if let Ok(track) = read_video_track(path) {
        if is_hap(&track.codec) {
            let r = HapReader::open(path)?;
            let n = track.samples.len() as u64;
            let info = ClipInfo {
                kind: DecoderKind::Hap,
                width: track.width,
                height: track.height,
                fps: track.fps(),
                frame_count: Some(n),
                bt709: false,
                duration: n as f64 / track.fps(),
                random_access: true,
                has_audio: false,
            };
            return Ok((Source::Hap(r), info));
        }
    }
    // A device without video support (WARP, odd drivers) → retry with MF software decode.
    let mf = MfVideo::open(path, device).or_else(|e| if device.is_some() { MfVideo::open(path, None) } else { Err(e) });
    let mf_err = match mf {
        Ok(v) => {
            let info = ClipInfo {
                kind: DecoderKind::MediaFoundation,
                width: v.width,
                height: v.height,
                fps: v.fps,
                frame_count: None,
                bt709: v.bt709,
                duration: v.duration,
                random_access: false,
                has_audio: false,
            };
            return Ok((Source::Mf(v), info));
        }
        Err(e) => e,
    };
    if crate::audio::has_audio(path) {
        let duration = crate::audio::AudioDecoder::open(path, 48_000).map_or(0.0, |d| d.duration);
        let info = ClipInfo {
            kind: DecoderKind::Audio,
            width: 0,
            height: 0,
            fps: 30.0,
            frame_count: None,
            bt709: false,
            duration,
            random_access: false,
            has_audio: true,
        };
        return Ok((Source::AudioOnly, info));
    }
    #[cfg(feature = "ffmpeg")]
    if let Ok(v) = crate::ff::FfVideo::open(path) {
        let info = ClipInfo {
            kind: DecoderKind::Ffmpeg,
            width: v.width,
            height: v.height,
            fps: v.fps,
            frame_count: None,
            bt709: false,
            duration: v.duration,
            random_access: false,
            has_audio: false,
        };
        return Ok((Source::Ff(v), info));
    }
    bail!("no decoder for {}: {mf_err:#}", path.display())
}

/// Opens `path` on its own decoder thread. `looping` only affects sequential sources.
pub fn open_clip(path: &Path, device: Option<ID3D11Device>, looping: bool) -> Result<ClipPlayer> {
    open_clip_at(path, device, looping, 0.0)
}

/// Where a clip starts: seconds, or a fraction of its length (in points: the length is only
/// known once the file is open).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StartAt {
    Secs(f64),
    Fraction(f64),
}

impl StartAt {
    pub fn secs(self, duration: f64) -> f64 {
        match self {
            StartAt::Secs(s) => s.max(0.0),
            StartAt::Fraction(f) => (f.clamp(0.0, 1.0) * duration.max(0.0)).max(0.0),
        }
    }
}

/// Like [`open_clip`], sequential sources starting at `start` seconds.
pub fn open_clip_at(path: &Path, device: Option<ID3D11Device>, looping: bool, start: f64) -> Result<ClipPlayer> {
    open_clip_from(path, device, looping, StartAt::Secs(start))
}

/// Like [`open_clip`], sequential sources starting at `start`.
pub fn open_clip_from(path: &Path, device: Option<ID3D11Device>, looping: bool, start: StartAt) -> Result<ClipPlayer> {
    let (info_tx, info_rx) = sync_channel::<Result<ClipInfo>>(1);
    // Small on purpose: the MF surface pool is small, holding more GPU samples stalls the decoder.
    let (tx, frames) = sync_channel(3);
    let stop = Arc::new(AtomicBool::new(false));
    let request = Arc::new(Request { index: AtomicU64::new(NO_REQUEST), backward: AtomicBool::new(false) });
    let (path, device, stop2, request2) = (path.to_path_buf(), SendDevice(device), stop.clone(), request.clone());
    let thread = std::thread::Builder::new().name("evj-decoder".into()).spawn(move || {
        let device = device;
        let ctx = Ctx { info_tx: &info_tx, tx: &tx, stop: &stop2, request: &request2, looping, start };
        let result = catch_unwind(AssertUnwindSafe(|| decode_loop(&path, device.0.as_ref(), &ctx)));
        let err = match result {
            Ok(Ok(())) => return,
            Ok(Err(e)) => e,
            Err(_) => anyhow!("decoder panicked"),
        };
        // Whichever side is still listening gets the error.
        let _ = info_tx.try_send(Err(anyhow!("{err:#}")));
        let _ = tx.send(Err(err));
    })?;
    let info = info_rx.recv().map_err(|_| anyhow!("decoder thread exited"))??;
    Ok(ClipPlayer { info, frames, request, stop, thread: Some(thread) })
}

struct Ctx<'a> {
    info_tx: &'a SyncSender<Result<ClipInfo>>,
    tx: &'a SyncSender<Result<Msg>>,
    stop: &'a AtomicBool,
    request: &'a Request,
    looping: bool,
    start: StartAt,
}

impl Ctx<'_> {
    fn running(&self) -> bool {
        !self.stop.load(Ordering::Relaxed)
    }
    /// false once the player is gone.
    fn send(&self, msg: Msg) -> bool {
        self.tx.send(Ok(msg)).is_ok()
    }
    fn idle(&self) {
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn decode_loop(path: &Path, device: Option<&ID3D11Device>, ctx: &Ctx) -> Result<()> {
    mf_init_thread()?;
    let (source, mut info) = open_source(path, device)?;
    if info.kind != DecoderKind::Image && info.kind != DecoderKind::Audio {
        info.has_audio = crate::audio::has_audio(path);
    }
    let _ = ctx.info_tx.send(Ok(info.clone()));
    match source {
        Source::Image(mut img) => {
            if let Some((_, _, data)) = img.take() {
                if !ctx.send(Msg::Frame(Frame { index: 0, pts: 0.0, data: FrameData::Bgra { data } })) {
                    return Ok(());
                }
            }
            while ctx.running() {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(())
        }
        Source::Hap(mut r) => random_access_loop(&mut r, &info, ctx),
        Source::AudioOnly => {
            while ctx.running() {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(())
        }
        Source::Mf(mut v) => {
            let start = ctx.start.secs(info.duration);
            if start > 0.0 {
                v.seek(start)?;
            }
            sequential_loop(&mut v, &info, ctx)
        }
        #[cfg(feature = "ffmpeg")]
        Source::Ff(mut v) => sequential_loop(&mut v, &info, ctx),
    }
}

fn random_access_loop(r: &mut HapReader, info: &ClipInfo, ctx: &Ctx) -> Result<()> {
    let n = info.frame_count.unwrap_or(1).max(1);
    let frame_dur = 1.0 / info.fps;
    let mut sent: VecDeque<u64> = VecDeque::new();
    let mut last_want = NO_REQUEST;
    while ctx.running() {
        let want = ctx.request.index.load(Ordering::Relaxed);
        if want == NO_REQUEST {
            ctx.idle();
            continue;
        }
        let want = want % n;
        if want != last_want && !sent.contains(&want) {
            sent.clear(); // jumped: earlier look-ahead is stale
        }
        last_want = want;
        let back = ctx.request.backward.load(Ordering::Relaxed);
        let next = (0..=LOOK_AHEAD.min(n - 1))
            .map(|k| if back { (want + n - k) % n } else { (want + k) % n })
            .find(|i| !sent.contains(i));
        let Some(index) = next else {
            ctx.idle();
            continue;
        };
        let mut data = Vec::new(); // ponytail: per-frame allocation, pool buffers if it shows in profiles
        let format = r.read_frame(index as usize, &mut data)?;
        if !ctx.send(Msg::Frame(Frame { index, pts: index as f64 * frame_dur, data: FrameData::Bc { format, data } })) {
            break;
        }
        sent.push_back(index);
        if sent.len() > 2 * LOOK_AHEAD as usize + 2 {
            sent.pop_front();
        }
    }
    Ok(())
}

/// Forward-only decoders.
trait Sequential {
    fn next(&mut self) -> Result<Option<(FrameData, f64)>>;
    fn rewind(&mut self) -> Result<()>;
}

impl Sequential for MfVideo {
    fn next(&mut self) -> Result<Option<(FrameData, f64)>> {
        Ok(self.next_frame()?.map(|(f, pts)| {
            let data = match f {
                VideoFrame::Gpu { texture, subresource, sample } => FrameData::Gpu { texture, subresource, sample },
                VideoFrame::Cpu { data } => FrameData::Bgra { data },
            };
            (data, pts)
        }))
    }
    fn rewind(&mut self) -> Result<()> {
        MfVideo::rewind(self)
    }
}

#[cfg(feature = "ffmpeg")]
impl Sequential for crate::ff::FfVideo {
    fn next(&mut self) -> Result<Option<(FrameData, f64)>> {
        let mut data = Vec::new();
        Ok(self.next_frame(&mut data)?.map(|pts| (FrameData::Bgra { data }, pts)))
    }
    fn rewind(&mut self) -> Result<()> {
        crate::ff::FfVideo::rewind(self)
    }
}

/// Forward-only decoding with loop offset so `pts` keeps increasing across loops.
fn sequential_loop(s: &mut impl Sequential, info: &ClipInfo, ctx: &Ctx) -> Result<()> {
    let frame_dur = 1.0 / info.fps;
    let (mut index, mut loop_offset, mut last_pts) = (0u64, 0.0, 0.0);
    while ctx.running() {
        match s.next()? {
            Some((data, pts)) => {
                last_pts = pts;
                if !ctx.send(Msg::Frame(Frame { index, pts: loop_offset + pts, data })) {
                    break;
                }
                index += 1;
            }
            None if index == 0 => bail!("video has no frames"),
            None if ctx.looping => {
                loop_offset += last_pts + frame_dur;
                s.rewind()?;
            }
            None => {
                if !ctx.send(Msg::End) {
                    break;
                }
                while ctx.running() {
                    std::thread::sleep(Duration::from_millis(20));
                }
            }
        }
    }
    Ok(())
}
