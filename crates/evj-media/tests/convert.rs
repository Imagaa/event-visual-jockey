use evj_media::convert::{HapVariant, convert_to_hap, find_ffmpeg};
use evj_media::hap::{HapFormat, HapReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn tmp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("evj-conv-{}-{name}", std::process::id()))
}

#[test]
fn converts_h264_to_hap_with_progress() {
    let ffmpeg = find_ffmpeg().expect("ffmpeg.exe next to the exe or on PATH");
    let out = tmp("a.mov");
    let mut last = 0.0;
    convert_to_hap(&ffmpeg, &fixture("h264_320x240.mp4"), &out, HapVariant::Hap, 1.0, &AtomicBool::new(false), |p| last = p).unwrap();
    assert!(last > 0.8, "progress reported: {last}");
    let mut r = HapReader::open(&out).unwrap();
    assert_eq!((r.track.width, r.track.height), (320, 240));
    assert_eq!(r.track.samples.len(), 30);
    assert_eq!(r.read_frame(0, &mut Vec::new()).unwrap(), HapFormat::Bc1);
}

#[test]
fn odd_sizes_are_padded_and_hap_q_works() {
    let ffmpeg = find_ffmpeg().unwrap();
    let odd = tmp("odd.avi");
    let st = std::process::Command::new(&ffmpeg)
        .args(["-y", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=66x50:rate=10", "-frames:v", "5", "-c:v", "mjpeg"])
        .arg(&odd)
        .status()
        .unwrap();
    assert!(st.success());
    let out = tmp("odd.mov");
    convert_to_hap(&ffmpeg, &odd, &out, HapVariant::HapQ, 0.5, &AtomicBool::new(false), |_| {}).unwrap();
    let mut r = HapReader::open(&out).unwrap();
    assert_eq!((r.track.width, r.track.height), (68, 52));
    assert_eq!(r.read_frame(0, &mut Vec::new()).unwrap(), HapFormat::YCoCgBc3);
}

#[test]
fn bad_input_reports_ffmpeg_error_and_cancel_stops() {
    let ffmpeg = find_ffmpeg().unwrap();
    let err = convert_to_hap(&ffmpeg, &fixture("garbage.mov"), &tmp("bad.mov"), HapVariant::Hap, 1.0, &AtomicBool::new(false), |_| {}).unwrap_err();
    assert!(!err.to_string().is_empty());
    let cancelled = convert_to_hap(&ffmpeg, &fixture("h264_320x240.mp4"), &tmp("c.mov"), HapVariant::Hap, 1.0, &AtomicBool::new(true), |_| {});
    assert!(cancelled.unwrap_err().to_string().contains("cancel"));
}
