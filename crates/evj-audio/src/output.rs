//! Audio output (WASAPI via cpal) and one decoder thread per playing voice.
use crate::mixer::{CHANNELS, Cmd, Meters, Mixer, Voice, VoiceHandle};
use anyhow::{Context, Result, anyhow};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use evj_media::audio::AudioDecoder;
use evj_media::mf::mf_init_thread;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::Arc;
use std::time::Duration;

/// Seconds of decoded audio buffered per voice.
const RING_SECS: f32 = 0.5;

struct Playing {
    handle: VoiceHandle,
    path: PathBuf,
    gain: f32,
    start_at: f64,
    looping: bool,
    stop: Arc<AtomicBool>,
    stopping: bool,
}

/// Where an output plays: a device (None = Windows default) and its first channel (0-based:
/// 0 = channels 1-2, 2 = 3-4 …).
#[derive(Clone, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Route {
    pub device: Option<String>,
    pub first_channel: u16,
}

/// First channels a device offers for a stereo pair: 0, 2, 4 … (mono / stereo: just 0).
pub fn channel_pairs(channels: u16) -> Vec<u16> {
    if channels <= 2 { vec![0] } else { (0..channels / 2).map(|p| p * 2).collect() }
}

/// One output frame: the stereo pair at `first`, every other channel silent; a mono device
/// gets the mix.
pub fn route_frame(frame: &mut [f32], first: usize, l: f32, r: f32) {
    if frame.len() == 1 {
        frame[0] = 0.5 * (l + r);
        return;
    }
    for (c, s) in frame.iter_mut().enumerate() {
        *s = if c == first { l } else if c == first + 1 { r } else { 0.0 };
    }
}

pub struct AudioEngine {
    route: Route,
    /// Program: a lost device falls back to the Windows default. Preview: it goes silent.
    fallback: bool,
    stream: cpal::Stream,
    tx: Sender<Cmd>,
    rate: u32,
    device: String,
    meters: Arc<Meters>,
    failed: Arc<AtomicBool>,
    master: f32,
    voices: Vec<Playing>,
}

fn open_stream(route: &Route, fallback: bool) -> Result<(cpal::Stream, Sender<Cmd>, u32, String, Arc<Meters>, Arc<AtomicBool>)> {
    let host = cpal::default_host();
    let named = route.device.as_deref().and_then(|w| host.output_devices().ok()?.find(|d| d.description().is_ok_and(|n| n.name() == w)));
    let device = match (named, &route.device) {
        (Some(d), _) => d,
        (None, Some(w)) if !fallback => anyhow::bail!("{w} not found"),
        _ => host.default_output_device().context("no audio output device")?,
    };
    let name = device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "Audio device".into());
    let mut config = device.default_output_config().map_err(|e| anyhow!("{e}"))?.config();
    config.buffer_size = cpal::BufferSize::Default;
    let (rate, dev_channels) = (config.sample_rate, config.channels as usize);
    let first = route.first_channel as usize;
    if (dev_channels == 1 && first > 0) || (dev_channels > 1 && first + 2 > dev_channels) {
        anyhow::bail!("{name} has only {dev_channels} channels");
    }
    let (tx, rx) = channel();
    let mut mixer = Mixer::new(rate, rx);
    let meters = mixer.meters();
    let failed = Arc::new(AtomicBool::new(false));
    let f2 = failed.clone();
    let mut stereo = vec![0.0f32; 8192 * CHANNELS];
    let stream = device
        .build_output_stream::<f32, _, _>(
            config,
            move |out: &mut [f32], _| {
                let frames = out.len() / dev_channels.max(1);
                if stereo.len() < frames * CHANNELS {
                    stereo.resize(frames * CHANNELS, 0.0); // rare: a bigger buffer than ever before
                }
                let buf = &mut stereo[..frames * CHANNELS];
                mixer.render(buf);
                for (f, frame) in out.chunks_mut(dev_channels.max(1)).enumerate() {
                    route_frame(frame, first, buf[f * 2], buf[f * 2 + 1]);
                }
            },
            move |e| {
                evj_core::log::warn("audio", &format!("stream error: {e}"));
                f2.store(true, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| anyhow!("cannot open {name}: {e}"))?;
    stream.play().map_err(|e| anyhow!("cannot start {name}: {e}"))?;
    Ok((stream, tx, rate, name, meters, failed))
}

fn decode(path: &Path, rate: u32, start_at: f64, looping: bool, mut ring: rtrb::Producer<f32>, stop: &AtomicBool) -> Result<()> {
    mf_init_thread()?;
    let mut d = AudioDecoder::open(path, rate)?;
    if start_at > 0.0 {
        d.seek(start_at)?;
    }
    while !stop.load(Ordering::Relaxed) {
        match d.read()? {
            Some((chunk, _)) => {
                let mut i = 0;
                while i < chunk.len() {
                    if stop.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    match ring.push(chunk[i]) {
                        Ok(()) => i += 1,
                        Err(_) => std::thread::sleep(Duration::from_millis(5)), // full: the mixer catches up
                    }
                }
            }
            None if looping => d.seek(0.0)?,
            None => break,
        }
    }
    Ok(())
}

impl AudioEngine {
    /// Opens `device` (by name) or the default output, channels 1-2.
    pub fn start(device: Option<&str>) -> Result<AudioEngine> {
        AudioEngine::start_route(&Route { device: device.map(Into::into), first_channel: 0 }, true)
    }

    /// Opens `route`. `fallback`: a missing / lost device means the Windows default (Program);
    /// otherwise it is an error / silence (Preview must never reach the sound system).
    pub fn start_route(route: &Route, fallback: bool) -> Result<AudioEngine> {
        let (stream, tx, rate, device, meters, failed) = open_stream(route, fallback)?;
        Ok(AudioEngine { route: route.clone(), fallback, stream, tx, rate, device, meters, failed, master: 1.0, voices: Vec::new() })
    }

    /// Output devices Windows offers: (name, channels).
    pub fn devices() -> Vec<(String, u16)> {
        let host = cpal::default_host();
        let list = host.output_devices().map(|ds| {
            ds.filter_map(|d| {
                let name = d.description().ok()?.name().to_string();
                Some((name, d.default_output_config().map_or(2, |c| c.channels())))
            })
            .collect()
        });
        list.unwrap_or_default()
    }

    /// Name of the Windows default output device.
    pub fn default_device() -> Option<String> {
        cpal::default_host().default_output_device()?.description().ok().map(|d| d.name().to_string())
    }

    pub fn route(&self) -> &Route {
        &self.route
    }

    /// The stream was lost and (no fallback) stays silent.
    pub fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    pub fn master_peak(&self) -> f32 {
        self.meters.master_peak()
    }

    pub fn master_peaks(&self) -> [f32; 2] {
        self.meters.master_peaks()
    }

    /// The CLIP lamp: output reached 0 dBFS since the last reset.
    pub fn master_clip(&self) -> bool {
        self.meters.clipped()
    }

    pub fn reset_clip(&self) {
        self.meters.reset_clip();
    }

    /// Audio gaps since the output was opened (see [`Meters::underruns`]).
    pub fn underruns(&self) -> u64 {
        self.meters.underruns()
    }

    /// Starts `path` at `start_at` seconds. Never blocks: open errors show up on the handle.
    pub fn play(&mut self, id: u64, path: &Path, gain: f32, start_at: f64, looping: bool) -> Result<VoiceHandle> {
        self.spawn(None, id, path, gain, start_at, looping)
    }

    fn spawn(&mut self, resume: Option<&VoiceHandle>, id: u64, path: &Path, gain: f32, start_at: f64, looping: bool) -> Result<VoiceHandle> {
        let (producer, consumer) = rtrb::RingBuffer::new((self.rate as f32 * RING_SECS) as usize * CHANNELS);
        let (voice, handle) = match resume {
            Some(h) => (Voice::resume(h, consumer, gain), h.clone()),
            None => Voice::new(id, consumer, gain),
        };
        self.tx.send(Cmd::Add(voice)).map_err(|_| anyhow!("audio stream stopped"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let (p, h, s, rate) = (path.to_path_buf(), handle.clone(), stop.clone(), self.rate);
        std::thread::Builder::new().name("evj-audio-decode".into()).spawn(move || {
            if let Err(e) = decode(&p, rate, start_at, looping, producer, &s) {
                h.set_error(format!("{e:#}"));
            }
            h.set_decoding_done();
        })?;
        if resume.is_none() {
            self.voices.push(Playing { handle: handle.clone(), path: path.to_path_buf(), gain, start_at, looping, stop, stopping: false });
        } else if let Some(v) = self.voices.iter_mut().find(|v| v.handle.id == id) {
            v.stop = stop;
        }
        Ok(handle)
    }

    pub fn set_gain(&mut self, id: u64, gain: f32, secs: f32) {
        let _ = self.tx.send(Cmd::Gain { id, gain, secs });
        for v in self.voices.iter_mut().filter(|v| v.handle.id == id) {
            v.gain = gain; // kept for a device change
        }
    }

    pub fn stop(&mut self, id: u64, secs: f32) {
        let _ = self.tx.send(Cmd::Stop { id, secs });
        for v in self.voices.iter_mut().filter(|v| v.handle.id == id && !v.stopping) {
            v.stopping = true;
            let stop = v.stop.clone();
            // the decoder may be waiting on a full ring: let it go once the fade is over
            let delay = Duration::from_secs_f32(secs.max(0.0) + 0.1);
            let _ = std::thread::Builder::new().spawn(move || {
                std::thread::sleep(delay);
                stop.store(true, Ordering::Relaxed);
            });
        }
    }

    pub fn master(&mut self, gain: f32, secs: f32) {
        self.master = gain;
        let _ = self.tx.send(Cmd::Master { gain, secs });
    }

    pub fn panic(&mut self, secs: f32) {
        self.master = 0.0;
        let _ = self.tx.send(Cmd::Panic { secs });
    }

    /// Voices still playing and not fading out.
    pub fn playing_ids(&self) -> Vec<u64> {
        self.voices.iter().filter(|v| !v.stopping && !v.handle.finished()).map(|v| v.handle.id).collect()
    }

    /// Housekeeping: forgets finished voices; re-opens the default device if the current one vanished
    /// (voices resume where they were).
    pub fn poll(&mut self) {
        self.voices.retain(|v| {
            let done = v.handle.finished();
            if done {
                v.stop.store(true, Ordering::Relaxed);
            }
            !done
        });
        if !self.failed.load(Ordering::Relaxed) || !self.fallback {
            return;
        }
        let Ok((stream, tx, rate, device, meters, failed)) = open_stream(&Route::default(), true) else { return };
        evj_core::log::warn("audio", &format!("output device lost, switched to {device}"));
        let old: Vec<Playing> = self.voices.drain(..).filter(|v| !v.stopping).collect();
        self.voices = old.iter().map(|v| Playing { handle: v.handle.clone(), path: v.path.clone(), gain: v.gain, start_at: v.start_at, looping: v.looping, stop: v.stop.clone(), stopping: false }).collect();
        (self.stream, self.tx, self.rate, self.device, self.meters, self.failed) = (stream, tx, rate, device, meters, failed);
        let master = self.master;
        self.master(master, 0.0);
        for v in old {
            v.stop.store(true, Ordering::Relaxed);
            // ponytail: resumes at played position (old rate); drift of a few ms on a rate change is inaudible
            let at = v.start_at + v.handle.position_secs(rate);
            if let Err(e) = self.spawn(Some(&v.handle), v.handle.id, &v.path, v.gain, at, v.looping) {
                v.handle.set_error(format!("{e:#}"));
            }
        }
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        for v in &self.voices {
            v.stop.store(true, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stereo_lands_on_its_pair_and_the_rest_is_silent() {
        let mut f = [9.0f32; 4];
        route_frame(&mut f, 2, 0.25, -0.5);
        assert_eq!(f, [0.0, 0.0, 0.25, -0.5]);
        let mut f = [9.0f32; 2];
        route_frame(&mut f, 0, 0.25, -0.5);
        assert_eq!(f, [0.25, -0.5]);
    }

    #[test]
    fn mono_devices_get_the_mix() {
        let mut f = [9.0f32; 1];
        route_frame(&mut f, 0, 0.5, 0.25);
        assert_eq!(f, [0.375]);
    }

    #[test]
    fn channel_pairs_follow_the_device() {
        assert_eq!(channel_pairs(1), vec![0]);
        assert_eq!(channel_pairs(2), vec![0]);
        assert_eq!(channel_pairs(6), vec![0, 2, 4]);
        assert_eq!(channel_pairs(5), vec![0, 2]);
    }
}
