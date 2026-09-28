//! Audio waveforms for the timeline: peaks decoded on a worker thread, cached per file.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

pub const BINS: usize = 2000;

/// A file's waveform: still decoding, ready, or none (no sound / unreadable).
pub enum Wave<'a> {
    Pending,
    Ready(&'a [f32]),
    None,
}

/// Adds one decoded block (interleaved stereo, starting at `start` s) to the peaks of a
/// `duration`-second file — the whole file never sits in memory.
pub fn fold(out: &mut [f32], block: &[f32], start: f64, rate: u32, duration: f64) {
    let n = out.len();
    if n == 0 || duration <= 0.0 {
        return;
    }
    let ch = evj_media::audio::CHANNELS;
    for (i, f) in block.chunks(ch).enumerate() {
        let t = start + i as f64 / rate.max(1) as f64;
        let b = ((t / duration * n as f64) as usize).min(n - 1);
        let peak = f.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        out[b] = out[b].max(peak.min(1.0));
    }
}


pub struct Waveforms {
    tx: Sender<PathBuf>,
    rx: Receiver<(PathBuf, Option<Vec<f32>>)>,
    cache: HashMap<PathBuf, Option<Vec<f32>>>,
    asked: HashSet<PathBuf>,
}

impl Waveforms {
    pub fn start() -> Waveforms {
        let (tx, jobs) = channel::<PathBuf>();
        let (out, rx) = channel();
        let _ = std::thread::Builder::new().name("evj-waveform".into()).spawn(move || {
            let _ = evj_media::mf::mf_init_thread();
            for path in jobs {
                let peaks = decode(&path);
                if out.send((path, peaks)).is_err() {
                    break;
                }
            }
        });
        Waveforms { tx, rx, cache: HashMap::new(), asked: HashSet::new() }
    }

    /// Peaks of `path` (0..1, [`BINS`] values); None while decoding or when it has no sound.
    pub fn get(&mut self, path: &Path) -> Wave<'_> {
        if self.asked.insert(path.to_path_buf()) {
            let _ = self.tx.send(path.to_path_buf());
        }
        match self.cache.get(path) {
            Some(Some(b)) => Wave::Ready(b),
            Some(None) => Wave::None,
            None => Wave::Pending,
        }
    }

    pub fn poll(&mut self) {
        while let Ok((path, peaks)) = self.rx.try_recv() {
            self.cache.insert(path, peaks);
        }
    }
}

/// Whole file at 8 kHz stereo → peaks, block by block.
fn decode(path: &Path) -> Option<Vec<f32>> {
    const RATE: u32 = 8000;
    let mut d = evj_media::audio::AudioDecoder::open(path, RATE).ok()?;
    if d.duration <= 0.0 {
        // Unknown length (rare): collect, then bin.
        let mut all = Vec::new();
        while let Ok(Some((block, _))) = d.read() {
            all.extend_from_slice(&block);
        }
        return (!all.is_empty()).then(|| bins(&all, evj_media::audio::CHANNELS, BINS));
    }
    let (duration, mut out, mut any) = (d.duration, vec![0.0f32; BINS], false);
    while let Ok(Some((block, start))) = d.read() {
        fold(&mut out, &block, start, RATE, duration);
        any = true;
    }
    any.then_some(out)
}

/// Peak (0..1) of each of `n` equal slices of interleaved `samples`.
pub fn bins(samples: &[f32], channels: usize, n: usize) -> Vec<f32> {
    let ch = channels.max(1);
    let frames = samples.len() / ch;
    let mut out = vec![0.0f32; n];
    if frames == 0 || n == 0 {
        return out;
    }
    for (i, f) in samples.chunks(ch).enumerate() {
        let b = (i * n / frames).min(n - 1);
        let peak = f.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        out[b] = out[b].max(peak.min(1.0));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bins_hold_the_peak_of_each_slice() {
        let s = [0.1, -0.1, 0.9, -0.2, 0.0, 0.0, -0.5, 0.3]; // stereo: 4 frames
        assert_eq!(bins(&s, 2, 2), vec![0.9, 0.5]);
        assert!(bins(&[], 2, 4).iter().all(|v| *v == 0.0));
    }

    #[test]
    fn streamed_blocks_land_in_their_time_bins() {
        let mut out = vec![0.0; 4];
        // 4 s of audio at 2 Hz stereo: one block per second.
        for (sec, v) in [0.1f32, 0.9, 0.2, 0.5].iter().enumerate() {
            fold(&mut out, &[*v, -*v, 0.0, 0.0], sec as f64, 2, 4.0);
        }
        assert_eq!(out, vec![0.1, 0.9, 0.2, 0.5]);
    }

    #[test]
    fn a_file_without_sound_is_reported_not_pending_forever() {
        let mut w = Waveforms::start();
        let f = Path::new("C:/nope/missing.mp3");
        for _ in 0..500 {
            w.poll();
            if matches!(w.get(f), Wave::None) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("still pending");
    }

    #[test]
    fn decodes_a_real_file() {
        let f = Path::new(env!("CARGO_MANIFEST_DIR")).join("../evj-media/tests/fixtures/tone_44k_mono.mp3");
        let mut w = Waveforms::start();
        for _ in 0..500 {
            w.poll();
            if let Wave::Ready(b) = w.get(&f) {
                assert_eq!(b.len(), BINS);
                assert!(b.iter().any(|v| *v > 0.1));
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("no waveform");
    }
}
