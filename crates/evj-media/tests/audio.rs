use evj_media::audio::{AudioDecoder, has_audio};
use evj_media::mf::mf_init_thread;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn read_all(d: &mut AudioDecoder) -> Vec<f32> {
    let mut all = Vec::new();
    while let Some((chunk, _pts)) = d.read().unwrap() {
        all.extend(chunk);
    }
    all
}

#[test]
fn mp3_is_resampled_to_the_device_rate_in_stereo() {
    mf_init_thread().unwrap();
    let mut d = AudioDecoder::open(&fixture("tone_44k_mono.mp3"), 48000).unwrap();
    assert_eq!(d.rate, 48000);
    assert!((d.duration - 1.0).abs() < 0.1, "duration {}", d.duration);
    let s = read_all(&mut d);
    let frames = s.len() / 2;
    assert!((frames as i64 - 48000).abs() < 3000, "{frames} frames");
    let peak = s.iter().fold(0f32, |m, v| m.max(v.abs()));
    assert!(peak > 0.1 && peak < 0.15, "peak {peak} (ffmpeg sine = 1/8 amplitude)");
    // mono upmix: both channels carry the tone
    assert!(s.chunks(2).skip(1000).take(100).all(|f| (f[0] - f[1]).abs() < 1e-3));
}

#[test]
fn seek_and_rewind() {
    mf_init_thread().unwrap();
    let mut d = AudioDecoder::open(&fixture("tone_44k_mono.mp3"), 48000).unwrap();
    d.seek(0.5).unwrap();
    let rest = read_all(&mut d).len() / 2;
    assert!(rest > 18000 && rest < 30000, "{rest} frames after seeking to 0.5 s");
    d.seek(0.0).unwrap();
    assert!(read_all(&mut d).len() / 2 > 44000);
}

#[test]
fn video_files_with_and_without_audio() {
    mf_init_thread().unwrap();
    assert!(has_audio(&fixture("av_320x240.mp4")));
    assert!(!has_audio(&fixture("h264_320x240.mp4")));
    assert!(!has_audio(&fixture("garbage.mov")));
    let mut d = AudioDecoder::open(&fixture("av_320x240.mp4"), 44100).unwrap();
    assert_eq!(d.rate, 44100);
    assert!(read_all(&mut d).len() / 2 > 40000);
}

#[test]
fn missing_file_is_error() {
    mf_init_thread().unwrap();
    assert!(AudioDecoder::open(Path::new("C:/nope/a.mp3"), 48000).is_err());
}
