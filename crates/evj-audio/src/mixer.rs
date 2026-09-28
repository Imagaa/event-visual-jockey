//! The realtime mixer: runs inside the audio callback, never blocks, never allocates per sample.
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Receiver;

pub const CHANNELS: usize = 2;

/// A gain that moves linearly to its target (no clicks).
#[derive(Clone, Copy)]
struct Ramp {
    value: f32,
    target: f32,
    step: f32,
}

impl Ramp {
    fn new(v: f32) -> Ramp {
        Ramp { value: v, target: v, step: 0.0 }
    }
    fn to(&mut self, target: f32, frames: f32) {
        self.target = target;
        if frames < 1.0 {
            self.value = target;
            self.step = 0.0;
        } else {
            self.step = (target - self.value) / frames;
        }
    }
    fn next(&mut self) -> f32 {
        if self.step != 0.0 {
            self.value += self.step;
            if (self.step > 0.0 && self.value >= self.target) || (self.step < 0.0 && self.value <= self.target) {
                self.value = self.target;
                self.step = 0.0;
            }
        }
        self.value
    }
}

/// Shared between the mixer and whoever started the voice.
#[derive(Default)]
struct Shared {
    position: AtomicU64,
    finished: AtomicBool,
    decoding_done: AtomicBool,
    peaks: [AtomicU32; 2],
    error: Mutex<Option<String>>,
}

/// Read side of a voice for the engine / UI: position (audio clock), end, level.
#[derive(Clone)]
pub struct VoiceHandle {
    pub id: u64,
    shared: Arc<Shared>,
}

impl VoiceHandle {
    pub fn position_frames(&self) -> u64 {
        self.shared.position.load(Ordering::Relaxed)
    }
    pub fn position_secs(&self, rate: u32) -> f64 {
        self.position_frames() as f64 / rate as f64
    }
    pub fn finished(&self) -> bool {
        self.shared.finished.load(Ordering::Relaxed)
    }
    /// Peak of the last rendered block (0..1), louder channel.
    pub fn peak(&self) -> f32 {
        let [l, r] = self.peaks();
        l.max(r)
    }
    /// Peak of the last rendered block per channel (left, right).
    pub fn peaks(&self) -> [f32; 2] {
        [0, 1].map(|c| f32::from_bits(self.shared.peaks[c].load(Ordering::Relaxed)))
    }
    /// The decoder has pushed its last sample (the voice ends when the ring runs empty).
    pub fn set_decoding_done(&self) {
        self.shared.decoding_done.store(true, Ordering::Relaxed);
    }
    pub fn set_error(&self, e: String) {
        if let Ok(mut g) = self.shared.error.lock() {
            *g = Some(e);
        }
    }

    /// Why the voice could not play (file missing, no audio track, ...).
    pub fn error(&self) -> Option<String> {
        self.shared.error.lock().ok().and_then(|g| g.clone())
    }
}

pub struct Voice {
    id: u64,
    ring: rtrb::Consumer<f32>,
    gain: Ramp,
    stopping: bool,
    shared: Arc<Shared>,
}

impl Voice {
    pub fn new(id: u64, ring: rtrb::Consumer<f32>, gain: f32) -> (Voice, VoiceHandle) {
        let shared = Arc::new(Shared::default());
        (Voice { id, ring, gain: Ramp::new(gain), stopping: false, shared: shared.clone() }, VoiceHandle { id, shared })
    }

    /// Continues an existing voice on a new ring (after the output device changed): same handle,
    /// the position keeps counting.
    pub fn resume(handle: &VoiceHandle, ring: rtrb::Consumer<f32>, gain: f32) -> Voice {
        handle.shared.decoding_done.store(false, Ordering::Relaxed);
        handle.shared.finished.store(false, Ordering::Relaxed);
        Voice { id: handle.id, ring, gain: Ramp::new(gain), stopping: false, shared: handle.shared.clone() }
    }
}

pub enum Cmd {
    Add(Voice),
    Gain { id: u64, gain: f32, secs: f32 },
    /// Fade out then drop the voice.
    Stop { id: u64, secs: f32 },
    Master { gain: f32, secs: f32 },
    /// Everything to silence, fast.
    Panic { secs: f32 },
}

#[derive(Default)]
pub struct Meters {
    master: [AtomicU32; 2],
    clip: AtomicBool,
    underruns: AtomicU64,
}

impl Meters {
    pub fn master_peak(&self) -> f32 {
        let [l, r] = self.master_peaks();
        l.max(r)
    }
    pub fn master_peaks(&self) -> [f32; 2] {
        [0, 1].map(|c| f32::from_bits(self.master[c].load(Ordering::Relaxed)))
    }
    /// A sample reached full scale since the last reset (the CLIP lamp).
    pub fn clipped(&self) -> bool {
        self.clip.load(Ordering::Relaxed)
    }
    pub fn reset_clip(&self) {
        self.clip.store(false, Ordering::Relaxed);
    }
    /// Callbacks in which a playing voice ran out of decoded sound (an audible gap).
    pub fn underruns(&self) -> u64 {
        self.underruns.load(Ordering::Relaxed)
    }
}

pub struct Mixer {
    rate: f32,
    voices: Vec<Voice>,
    master: Ramp,
    rx: Receiver<Cmd>,
    meters: Arc<Meters>,
}

impl Mixer {
    pub fn new(rate: u32, rx: Receiver<Cmd>) -> Mixer {
        Mixer { rate: rate as f32, voices: Vec::with_capacity(64), master: Ramp::new(1.0), rx, meters: Arc::new(Meters::default()) }
    }

    pub fn meters(&self) -> Arc<Meters> {
        self.meters.clone()
    }

    pub fn voice_count(&self) -> usize {
        self.voices.len()
    }

    fn command(&mut self, c: Cmd) {
        let frames = |secs: f32| secs.max(0.0) * self.rate;
        match c {
            Cmd::Add(v) => self.voices.push(v),
            Cmd::Gain { id, gain, secs } => {
                let f = frames(secs);
                if let Some(v) = self.voices.iter_mut().find(|v| v.id == id && !v.stopping) {
                    v.gain.to(gain.max(0.0), f);
                }
            }
            Cmd::Stop { id, secs } => {
                let f = frames(secs);
                for v in self.voices.iter_mut().filter(|v| v.id == id) {
                    v.stopping = true;
                    v.gain.to(0.0, f);
                }
            }
            Cmd::Master { gain, secs } => {
                let f = frames(secs);
                self.master.to(gain.max(0.0), f);
            }
            Cmd::Panic { secs } => {
                let f = frames(secs);
                self.master.to(0.0, f);
            }
        }
    }

    /// Fills interleaved stereo `out` (the audio callback).
    pub fn render(&mut self, out: &mut [f32]) {
        while let Ok(c) = self.rx.try_recv() {
            self.command(c);
        }
        out.fill(0.0);
        let frames = out.len() / CHANNELS;
        for v in &mut self.voices {
            let mut played = 0u64;
            let mut peak = [0f32; 2];
            for f in 0..frames {
                let g = v.gain.next();
                if v.ring.slots() < CHANNELS {
                    break; // underrun or end: silence
                }
                for c in 0..CHANNELS {
                    let s = v.ring.pop().unwrap_or(0.0) * g;
                    out[f * CHANNELS + c] += s;
                    peak[c] = peak[c].max(s.abs());
                }
                played += 1;
            }
            // Started, still decoding, not fading out, and the ring ran dry: the decoder fell behind.
            let started = played > 0 || v.shared.position.load(Ordering::Relaxed) > 0;
            if (played as usize) < frames && started && !v.stopping && !v.shared.decoding_done.load(Ordering::Relaxed) {
                self.meters.underruns.fetch_add(1, Ordering::Relaxed);
            }
            v.shared.position.fetch_add(played, Ordering::Relaxed);
            for c in 0..CHANNELS {
                v.shared.peaks[c].store(peak[c].to_bits(), Ordering::Relaxed);
            }
            let drained = v.shared.decoding_done.load(Ordering::Relaxed) && v.ring.slots() < CHANNELS;
            let faded = v.stopping && v.gain.value <= 0.0;
            if drained || faded {
                v.shared.finished.store(true, Ordering::Relaxed);
            }
        }
        self.voices.retain(|v| !v.shared.finished.load(Ordering::Relaxed));
        let mut peak = [0f32; 2];
        let mut clipped = false;
        for f in 0..frames {
            let g = self.master.next();
            for c in 0..CHANNELS {
                let raw = out[f * CHANNELS + c] * g;
                clipped |= raw.abs() >= 0.999;
                let s = raw.clamp(-1.0, 1.0);
                out[f * CHANNELS + c] = s;
                peak[c] = peak[c].max(s.abs());
            }
        }
        for c in 0..CHANNELS {
            self.meters.master[c].store(peak[c].to_bits(), Ordering::Relaxed);
        }
        if clipped {
            self.meters.clip.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    const RATE: u32 = 1000;

    /// A voice whose ring is pre-filled with `frames` stereo frames of `value`.
    fn voice(id: u64, value: f32, frames: usize) -> (Voice, VoiceHandle) {
        let (mut p, c) = rtrb::RingBuffer::new(frames * 2 + 16);
        for _ in 0..frames * 2 {
            p.push(value).unwrap();
        }
        let (v, h) = Voice::new(id, c, 1.0);
        h.set_decoding_done();
        (v, h)
    }

    fn mixer() -> (Mixer, std::sync::mpsc::Sender<Cmd>) {
        let (tx, rx) = channel();
        (Mixer::new(RATE, rx), tx)
    }

    #[test]
    fn mixes_voices_with_gain_and_limits() {
        let (mut m, tx) = mixer();
        let (a, _) = voice(1, 0.5, 100);
        let (b, _) = voice(2, 0.25, 100);
        tx.send(Cmd::Add(a)).unwrap();
        tx.send(Cmd::Add(b)).unwrap();
        let mut out = vec![0.0; 20];
        m.render(&mut out);
        assert!(out.iter().all(|s| (s - 0.75).abs() < 1e-6), "{out:?}");

        let (c, _) = voice(3, 0.9, 100);
        tx.send(Cmd::Add(c)).unwrap();
        m.render(&mut out);
        assert!(out.iter().all(|s| *s <= 1.0 && *s > 0.99), "limited to 1.0: {out:?}");
    }

    #[test]
    fn gain_ramps_linearly_without_jumps() {
        let (mut m, tx) = mixer();
        let (a, _) = voice(1, 1.0, 1000);
        tx.send(Cmd::Add(a)).unwrap();
        tx.send(Cmd::Gain { id: 1, gain: 0.0, secs: 0.1 }).unwrap(); // 100 frames
        let mut out = vec![0.0; 2 * 200];
        m.render(&mut out);
        let left: Vec<f32> = out.iter().step_by(2).copied().collect();
        assert!(left.windows(2).all(|w| (w[0] - w[1]).abs() <= 0.011), "no step bigger than 1/100");
        assert!(left[99] < 0.02 && left[150] == 0.0);
    }

    #[test]
    fn stop_fades_then_removes_the_voice() {
        let (mut m, tx) = mixer();
        let (a, h) = voice(1, 1.0, 1000);
        tx.send(Cmd::Add(a)).unwrap();
        tx.send(Cmd::Stop { id: 1, secs: 0.05 }).unwrap();
        let mut out = vec![0.0; 2 * 100];
        m.render(&mut out);
        assert!(h.finished());
        assert_eq!(m.voice_count(), 0);
        assert_eq!(out[2 * 99], 0.0);
    }

    #[test]
    fn position_counts_played_frames_and_underrun_is_silent() {
        let (mut m, tx) = mixer();
        let (a, h) = voice(1, 0.5, 30);
        tx.send(Cmd::Add(a)).unwrap();
        let mut out = vec![0.0; 2 * 50];
        m.render(&mut out);
        assert_eq!(h.position_frames(), 30);
        assert!((h.position_secs(RATE) - 0.03).abs() < 1e-9);
        assert_eq!(out[2 * 40], 0.0, "silence after the data ran out");
        assert!(h.finished(), "decoder done + ring empty = finished");
    }

    #[test]
    fn underruns_count_only_gaps_while_decoding() {
        let (mut m, tx) = mixer();
        let (p, c) = rtrb::RingBuffer::new(64);
        let (v, h) = Voice::new(1, c, 1.0);
        let mut p = p;
        tx.send(Cmd::Add(v)).unwrap();
        let mut out = vec![0.0; 2 * 10];
        m.render(&mut out);
        assert_eq!(m.meters().underruns(), 0, "not started yet: waiting for the decoder is no gap");
        for _ in 0..8 {
            p.push(0.5).unwrap();
        }
        m.render(&mut out);
        assert_eq!(m.meters().underruns(), 1, "started, then ran dry while decoding");
        h.set_decoding_done();
        m.render(&mut out);
        assert_eq!(m.meters().underruns(), 1, "the end of the file is not an underrun");
    }

    #[test]
    fn stereo_peaks_and_clip_latch() {
        let (mut m, tx) = mixer();
        let (mut p, c) = rtrb::RingBuffer::new(64);
        for _ in 0..8 {
            p.push(0.25).unwrap(); // left
            p.push(1.0).unwrap(); // right, full scale
        }
        let (v, h) = Voice::new(1, c, 1.0);
        h.set_decoding_done();
        tx.send(Cmd::Add(v)).unwrap();
        let mut out = vec![0.0; 2 * 8];
        m.render(&mut out);
        assert_eq!(h.peaks(), [0.25, 1.0]);
        let meters = m.meters();
        assert_eq!(meters.master_peaks(), [0.25, 1.0]);
        assert!(meters.clipped(), "full scale latches the clip lamp");
        m.render(&mut out); // silence now
        assert!(meters.clipped(), "stays lit until reset");
        meters.reset_clip();
        assert!(!meters.clipped());
    }

    #[test]
    fn master_gain_and_panic_fade() {
        let (mut m, tx) = mixer();
        let (a, _) = voice(1, 1.0, 1000);
        tx.send(Cmd::Add(a)).unwrap();
        tx.send(Cmd::Master { gain: 0.5, secs: 0.0 }).unwrap();
        let mut out = vec![0.0; 4];
        m.render(&mut out);
        assert!((out[0] - 0.5).abs() < 1e-6);
        tx.send(Cmd::Panic { secs: 0.01 }).unwrap();
        let mut out = vec![0.0; 2 * 20];
        m.render(&mut out);
        assert_eq!(out[2 * 19], 0.0);
        assert!(m.meters().master_peak() < 0.6);
    }

    #[test]
    fn meters_report_peaks() {
        let (mut m, tx) = mixer();
        let (a, h) = voice(1, 0.4, 100);
        tx.send(Cmd::Add(a)).unwrap();
        let mut out = vec![0.0; 20];
        m.render(&mut out);
        assert!((h.peak() - 0.4).abs() < 1e-6);
        assert!((m.meters().master_peak() - 0.4).abs() < 1e-6);
    }
}
