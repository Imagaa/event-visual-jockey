use evj_media::{ClipPlayer, DecoderKind, Frame, FrameData, Msg, open_clip};
use std::path::{Path, PathBuf};
use std::time::Duration;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn next_frame(p: &ClipPlayer) -> Frame {
    match p.frames.recv_timeout(Duration::from_secs(5)).expect("frame in time").expect("decode ok") {
        Msg::Frame(f) => f,
        Msg::End => panic!("unexpected end"),
    }
}

#[test]
fn hap_is_random_access_and_serves_requests() {
    let p = open_clip(&fixture("hap1_64x48.mov"), None, true).unwrap();
    assert_eq!(p.info.kind, DecoderKind::Hap);
    assert!(p.info.random_access);
    assert_eq!(p.info.frame_count, Some(3));
    assert!((p.info.duration - 0.1).abs() < 1e-6);
    p.request(2, true);
    let f = next_frame(&p);
    assert_eq!(f.index, 2);
    assert!(matches!(&f.data, FrameData::Bc { data, .. } if data.len() == 1536));
    // look-ahead wraps: 2 → 0 → 1
    assert_eq!(next_frame(&p).index, 0);
    assert_eq!(next_frame(&p).index, 1);
}

#[test]
fn hap_backwards_look_ahead() {
    let p = open_clip(&fixture("hap1_64x48.mov"), None, true).unwrap();
    p.request(1, false);
    assert_eq!(next_frame(&p).index, 1);
    assert_eq!(next_frame(&p).index, 0);
    assert_eq!(next_frame(&p).index, 2);
}

#[test]
fn nothing_is_decoded_before_a_request() {
    let p = open_clip(&fixture("hap1_64x48.mov"), None, true).unwrap();
    assert!(p.frames.recv_timeout(Duration::from_millis(100)).is_err());
}

#[test]
fn image_is_a_single_still_frame() {
    let p = open_clip(&fixture("red_64x36.png"), None, true).unwrap();
    assert_eq!(p.info.kind, DecoderKind::Image);
    assert_eq!((p.info.width, p.info.height), (64, 36));
    let f = next_frame(&p);
    assert!(matches!(&f.data, FrameData::Bgra { data } if data.len() == 64 * 36 * 4));
    assert!(p.frames.recv_timeout(Duration::from_millis(100)).is_err(), "only one frame");
}

#[test]
fn h264_is_sequential_with_duration_and_loops() {
    let p = open_clip(&fixture("h264_320x240.mp4"), None, true).unwrap();
    assert_eq!(p.info.kind, DecoderKind::MediaFoundation);
    assert!(!p.info.random_access);
    assert!((p.info.duration - 1.0).abs() < 0.05, "duration {}", p.info.duration);
    let mut last = -1.0;
    for _ in 0..40 {
        let f = next_frame(&p); // > 30 → looped
        assert!(f.pts > last, "pts must keep increasing across loops: {} after {last}", f.pts);
        last = f.pts;
    }
}

#[test]
fn h264_once_ends() {
    let p = open_clip(&fixture("h264_320x240.mp4"), None, false).unwrap();
    let mut frames = 0;
    loop {
        match p.frames.recv_timeout(Duration::from_secs(5)).unwrap().unwrap() {
            Msg::Frame(_) => frames += 1,
            Msg::End => break,
        }
    }
    assert_eq!(frames, 30);
}

#[test]
fn bad_inputs_are_errors_not_panics() {
    assert!(open_clip(&fixture("garbage.mov"), None, true).is_err());
    assert!(open_clip(Path::new("C:/nope/missing.mp4"), None, true).is_err());
    assert!(open_clip(Path::new("C:/nope/missing.png"), None, true).is_err());
}

#[test]
fn dropping_player_stops_thread() {
    let p = open_clip(&fixture("h264_320x240.mp4"), None, true).unwrap();
    drop(p); // must return (thread joined) without hanging
}

#[cfg(feature = "ffmpeg")]
#[test]
fn prores_falls_back_to_ffmpeg() {
    let p = open_clip(&fixture("prores_320x240.mov"), None, true).unwrap();
    assert!(matches!(p.info.kind, DecoderKind::Ffmpeg | DecoderKind::MediaFoundation));
    assert!(matches!(next_frame(&p).data, FrameData::Bgra { .. } | FrameData::Gpu { .. }));
}

#[test]
fn clips_report_their_audio() {
    let v = open_clip(&fixture("av_320x240.mp4"), None, true).unwrap();
    assert!(v.info.has_audio);
    let silent = open_clip(&fixture("h264_320x240.mp4"), None, true).unwrap();
    assert!(!silent.info.has_audio);
}

#[test]
fn audio_files_open_as_audio_only_clips() {
    let a = open_clip(&fixture("tone_44k_mono.mp3"), None, true).unwrap();
    assert_eq!(a.info.kind, DecoderKind::Audio);
    assert!(a.info.has_audio);
    assert!((a.info.duration - 1.0).abs() < 0.1, "{}", a.info.duration);
    assert!(a.frames.recv_timeout(Duration::from_millis(100)).is_err(), "no video frames");
}

#[test]
fn sequential_clip_can_start_at_a_fraction() {
    use evj_media::{StartAt, open_clip_from};
    let p = open_clip_from(&fixture("h264_320x240.mp4"), None, false, StartAt::Fraction(0.5)).unwrap();
    let first = next_frame(&p);
    assert!(first.pts >= 0.4, "starts near the middle, got {}", first.pts);
    assert_eq!(StartAt::Fraction(0.25).secs(8.0), 2.0);
    assert_eq!(StartAt::Secs(3.0).secs(8.0), 3.0);
}
