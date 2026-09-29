//! Background thumbnails + media info. Uses a WARP (software) device so it never touches the GPU the show runs on.
use anyhow::{Context, Result};
use evj_media::hap::{HapFormat, HapReader, is_hap};
use evj_media::image::{is_image, load_image};
use evj_media::mf::{MfVideo, VideoFrame, mf_init_thread};
use evj_media::mov::read_video_track;
use evj_media::{ClipInfo, DecoderKind};
use evj_render::{Blitter, DeviceKind, Gpu, RenderTarget, Shade, Texture, fit_rect};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use windows::Win32::Graphics::Dxgi::Common::*;

pub const W: u32 = 160;
pub const H: u32 = 90;

pub enum Thumb {
    Pending,
    Ready { tex: egui::TextureHandle, info: ClipInfo },
    Failed(String),
}

pub struct Thumbnailer {
    tx: Sender<PathBuf>,
    rx: Receiver<(PathBuf, Result<(ClipInfo, Vec<u8>)>)>,
    pub cache: HashMap<PathBuf, Thumb>,
}

impl Thumbnailer {
    pub fn start() -> Thumbnailer {
        let (tx, jobs) = channel::<PathBuf>();
        let (done, rx) = channel();
        let _ = std::thread::Builder::new().name("evj-thumbs".into()).spawn(move || {
            let _ = mf_init_thread();
            let Ok(gpu) = Gpu::new(DeviceKind::Warp) else { return };
            let Ok(blitter) = Blitter::new(&gpu) else { return };
            // Decoding a frame of every video (software, big 4K files) keeps several cores busy
            // for seconds: done once per file, then read from disk.
            let cache = evj_core::io::app_dir().map(|d| d.join("thumbs"));
            for path in jobs {
                let r = match cache.as_deref().and_then(|c| cache_load(c, &path)) {
                    Some(hit) => Ok(hit),
                    None => {
                        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| make(&gpu, &blitter, &path)))
                            .unwrap_or_else(|_| Err(anyhow::anyhow!("thumbnail crashed")));
                        if let (Some(c), Ok((info, px))) = (cache.as_deref(), &r) {
                            cache_save(c, &path, info, px);
                        }
                        r
                    }
                };
                if done.send((path, r)).is_err() {
                    break;
                }
            }
        });
        Thumbnailer { tx, rx, cache: HashMap::new() }
    }

    /// Thumbnail for `path`, queued on first ask.
    pub fn get(&mut self, path: &Path) -> &Thumb {
        if !self.cache.contains_key(path) {
            let _ = self.tx.send(path.to_path_buf());
            self.cache.insert(path.to_path_buf(), Thumb::Pending);
        }
        &self.cache[path]
    }

    pub fn info(&self, path: &Path) -> Option<&ClipInfo> {
        match self.cache.get(path) {
            Some(Thumb::Ready { info, .. }) => Some(info),
            _ => None,
        }
    }

    /// Moves finished thumbnails into egui textures.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((path, r)) = self.rx.try_recv() {
            let t = match r {
                Ok((info, rgba)) => {
                    let img = egui::ColorImage::from_rgba_unmultiplied([W as usize, H as usize], &rgba);
                    Thumb::Ready { tex: ctx.load_texture(path.to_string_lossy(), img, egui::TextureOptions::LINEAR), info }
                }
                Err(e) => Thumb::Failed(format!("{e:#}")),
            };
            self.cache.insert(path, t);
        }
    }
}

/// The cache file for `path` as it is now (a changed or moved file gets a new one).
fn cache_file(dir: &Path, path: &Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (path.to_string_lossy().to_lowercase(), meta.len(), mtime).hash(&mut h);
    Some(dir.join(format!("{:016x}.thumb", h.finish())))
}

const CACHE_MAGIC: &str = "EVJT1";

fn cache_save(dir: &Path, path: &Path, info: &ClipInfo, rgba: &[u8]) {
    let Some(file) = cache_file(dir, path) else { return };
    let frames = info.frame_count.map_or("-".to_string(), |n| n.to_string());
    let head = format!(
        "{CACHE_MAGIC} {:?} {} {} {} {frames} {} {} {} {}\n",
        info.kind, info.width, info.height, info.fps, info.bt709, info.duration, info.random_access, info.has_audio
    );
    let _ = std::fs::create_dir_all(dir);
    let _ = evj_core::io::write_atomic(&file, &[head.as_bytes(), rgba].concat());
}

fn cache_load(dir: &Path, path: &Path) -> Option<(ClipInfo, Vec<u8>)> {
    let bytes = std::fs::read(cache_file(dir, path)?).ok()?;
    let nl = bytes.iter().position(|b| *b == b'\n')?;
    let head = std::str::from_utf8(&bytes[..nl]).ok()?;
    let f: Vec<&str> = head.split(' ').collect();
    let [magic, kind, w, h, fps, frames, bt709, duration, random_access, has_audio] = f[..] else { return None };
    if magic != CACHE_MAGIC {
        return None;
    }
    let kind = match kind {
        "Hap" => DecoderKind::Hap,
        "Image" => DecoderKind::Image,
        "MediaFoundation" => DecoderKind::MediaFoundation,
        "Ffmpeg" => DecoderKind::Ffmpeg,
        "Audio" => DecoderKind::Audio,
        _ => return None,
    };
    let rgba = bytes[nl + 1..].to_vec();
    if rgba.len() != (W * H * 4) as usize {
        return None;
    }
    let info = ClipInfo {
        kind,
        width: w.parse().ok()?,
        height: h.parse().ok()?,
        fps: fps.parse().ok()?,
        frame_count: if frames == "-" { None } else { Some(frames.parse().ok()?) },
        bt709: bt709.parse().ok()?,
        duration: duration.parse().ok()?,
        random_access: random_access.parse().ok()?,
        has_audio: has_audio.parse().ok()?,
    };
    Some((info, rgba))
}

fn render(gpu: &Gpu, blitter: &Blitter, tex: &Texture, shade: Shade) -> Result<Vec<u8>> {
    let rt = RenderTarget::new(gpu, W, H)?;
    unsafe { gpu.ctx.ClearRenderTargetView(&rt.rtv, &[0.0, 0.0, 0.0, 1.0]) };
    blitter.draw(&gpu.ctx, tex, shade, &rt.rtv, fit_rect(tex.width, tex.height, W, H, 0));
    rt.readback(gpu)
}

fn make(gpu: &Gpu, blitter: &Blitter, path: &Path) -> Result<(ClipInfo, Vec<u8>)> {
    // A presentation (deck.json): its first slide.
    if evj_core::slides::SlideDeck::is_deck(path) {
        let deck = evj_core::slides::SlideDeck::load(path)?;
        let first = deck.slides.first().context("the presentation has no slides")?;
        return make(gpu, blitter, &first.image);
    }
    if is_image(path) {
        let (w, h, px) = load_image(path, 1024)?;
        let tex = Texture::new_color(gpu, DXGI_FORMAT_B8G8R8A8_UNORM, w, h)?;
        tex.upload(&gpu.ctx, &px, w * 4);
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
        return Ok((info, render(gpu, blitter, &tex, Shade::Rgba)?));
    }
    if let Ok(track) = read_video_track(path) {
        if is_hap(&track.codec) {
            let mut r = HapReader::open(path)?;
            let n = track.samples.len();
            let mut data = Vec::new();
            let format = r.read_frame(n / 4, &mut data)?;
            let (dxgi, shade) = match format {
                HapFormat::Bc1 => (DXGI_FORMAT_BC1_UNORM, Shade::Rgba),
                HapFormat::Bc3 => (DXGI_FORMAT_BC3_UNORM, Shade::Rgba),
                HapFormat::YCoCgBc3 => (DXGI_FORMAT_BC3_UNORM, Shade::YCoCg),
            };
            let tex = Texture::new_bc(gpu, dxgi, track.width, track.height)?;
            tex.upload(&gpu.ctx, &data, (track.width.div_ceil(4) as usize * format.block_bytes()) as u32);
            let info = ClipInfo {
                kind: DecoderKind::Hap,
                width: track.width,
                height: track.height,
                fps: track.fps(),
                frame_count: Some(n as u64),
                bt709: false,
                duration: n as f64 / track.fps(),
                random_access: true,
                has_audio: false,
            };
            return Ok((info, render(gpu, blitter, &tex, shade)?));
        }
    }
    let mut v = match MfVideo::open(path, None) {
        Ok(v) => v,
        Err(e) => {
            #[cfg(feature = "ffmpeg")]
            if let Ok(r) = ffmpeg_thumb(gpu, blitter, path) {
                return Ok(r);
            }
            if evj_media::audio::has_audio(path) {
                return Ok(audio_card(path));
            }
            return Err(e);
        }
    };
    v.seek(v.duration * 0.25)?;
    let (frame, _) = v.next_frame()?.context("video has no frames")?;
    let VideoFrame::Cpu { data } = frame else { anyhow::bail!("unexpected GPU frame") };
    let tex = Texture::new_color(gpu, DXGI_FORMAT_B8G8R8A8_UNORM, v.width, v.height)?;
    tex.upload(&gpu.ctx, &data, v.width * 4);
    let info = ClipInfo {
        kind: DecoderKind::MediaFoundation,
        width: v.width,
        height: v.height,
        fps: v.fps,
        frame_count: None,
        bt709: v.bt709,
        duration: v.duration,
        random_access: false,
        has_audio: evj_media::audio::has_audio(path),
    };
    Ok((info, render(gpu, blitter, &tex, Shade::Rgba)?))
}

/// Codecs Media Foundation cannot open (ProRes, DNxHD, ...): the first frame via FFmpeg.
#[cfg(feature = "ffmpeg")]
fn ffmpeg_thumb(gpu: &Gpu, blitter: &Blitter, path: &Path) -> Result<(ClipInfo, Vec<u8>)> {
    let mut v = evj_media::ff::FfVideo::open(path)?;
    let mut data = Vec::new();
    v.next_frame(&mut data)?.context("video has no frames")?;
    let tex = Texture::new_color(gpu, DXGI_FORMAT_B8G8R8A8_UNORM, v.width, v.height)?;
    tex.upload(&gpu.ctx, &data, v.width * 4);
    let info = ClipInfo {
        kind: DecoderKind::Ffmpeg,
        width: v.width,
        height: v.height,
        fps: v.fps,
        frame_count: None,
        bt709: false,
        duration: v.duration,
        random_access: false,
        has_audio: evj_media::audio::has_audio(path),
    };
    Ok((info, render(gpu, blitter, &tex, Shade::Rgba)?))
}

/// Thumbnail for sound-only clips: a stylised waveform.
fn audio_card(path: &Path) -> (ClipInfo, Vec<u8>) {
    let duration = evj_media::audio::AudioDecoder::open(path, 48_000).map_or(0.0, |d| d.duration);
    let mut px = vec![0u8; (W * H * 4) as usize];
    for x in 0..W {
        let h = ((((x as f32) * 0.37).sin() * 0.5 + ((x as f32) * 0.11).sin() * 0.5).abs() * 30.0 + 4.0) as u32;
        for y in 0..H {
            let on = y.abs_diff(H / 2) < h;
            let c = if on { [70, 190, 170, 255] } else { [22, 34, 38, 255] };
            px[((y * W + x) * 4) as usize..((y * W + x) * 4 + 4) as usize].copy_from_slice(&c);
        }
    }
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
    (info, px)
}

#[cfg(all(test, feature = "ffmpeg"))]
mod tests {
    use super::*;

    #[test]
    fn prores_thumbnail_comes_from_ffmpeg() {
        let gpu = Gpu::new(DeviceKind::Warp).unwrap();
        let blitter = Blitter::new(&gpu).unwrap();
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../evj-media/tests/fixtures/prores_320x240.mov");
        let (info, px) = make(&gpu, &blitter, &path).unwrap();
        assert_eq!(info.kind, DecoderKind::Ffmpeg);
        assert_eq!((info.width, info.height), (320, 240));
        assert!(px.chunks(4).any(|p| p[..3] != [0, 0, 0]), "a picture, not black");
    }
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[test]
    fn a_presentation_shows_its_first_slide() {
        let dir = std::env::temp_dir().join(format!("evj-deckthumb-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../evj-media/tests/fixtures");
        std::fs::copy(fixtures.join("red_64x36.png"), dir.join("slide001.png")).unwrap();
        std::fs::copy(fixtures.join("blue_36x64.jpg"), dir.join("slide002.jpg")).unwrap();
        let slide = |image: &str| evj_core::slides::Slide { image: image.into(), video: None, steps: vec![], end: 0.0, notes: String::new() };
        let deck = evj_core::slides::SlideDeck { title: "T".into(), source: "t.pdf".into(), width: 64, height: 36, slides: vec![slide("slide001.png"), slide("slide002.jpg")] };
        deck.save(&dir).unwrap();
        let gpu = Gpu::new(DeviceKind::Warp).unwrap();
        let blitter = Blitter::new(&gpu).unwrap();
        let (info, px) = make(&gpu, &blitter, &dir.join("deck.json")).expect("a thumbnail");
        assert_eq!(info.duration, 0.0);
        let mid = ((H / 2 * W + W / 2) * 4) as usize;
        assert!(px[mid] > 200 && px[mid + 2] < 60, "the red first slide: {:?}", &px[mid..mid + 4]);
    }

    fn info() -> ClipInfo {
        ClipInfo { kind: DecoderKind::MediaFoundation, width: 3840, height: 2160, fps: 25.0, frame_count: None, bt709: true, duration: 269.5, random_access: false, has_audio: true }
    }

    #[test]
    fn a_thumbnail_is_kept_on_disk_until_its_file_changes() {
        let dir = std::env::temp_dir().join(format!("evj-thumbcache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let media = dir.join("clip.mp4");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&media, b"first").unwrap();
        let px: Vec<u8> = (0..W * H * 4).map(|i| (i % 251) as u8).collect();
        assert!(cache_load(&dir, &media).is_none(), "nothing yet");
        cache_save(&dir, &media, &info(), &px);
        let (back, got) = cache_load(&dir, &media).expect("cached");
        assert_eq!(got, px);
        assert_eq!((back.kind, back.width, back.height, back.frame_count, back.bt709, back.random_access, back.has_audio), (DecoderKind::MediaFoundation, 3840, 2160, None, true, false, true));
        assert!((back.fps - 25.0).abs() < 1e-9 && (back.duration - 269.5).abs() < 1e-9);
        std::fs::write(&media, b"second, longer").unwrap();
        assert!(cache_load(&dir, &media).is_none(), "a changed file is thumbnailed again");
        assert!(cache_load(&dir, &dir.join("missing.mp4")).is_none());
    }
}
