use evj_media::mov::read_video_track;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn temp_with(bytes: &[u8], name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("evj-test-{}-{name}", std::process::id()));
    std::fs::write(&p, bytes).unwrap();
    p
}

#[test]
fn reads_hap_video_track() {
    let t = read_video_track(&fixture("hap1_64x48.mov")).unwrap();
    assert_eq!(&t.codec, b"Hap1");
    assert_eq!((t.width, t.height), (64, 48));
    assert_eq!(t.samples.len(), 3);
    assert!((t.fps() - 30.0).abs() < 0.01, "fps {}", t.fps());
    assert!(t.samples.windows(2).all(|w| w[1].offset >= w[0].offset + w[0].size as u64));
}

#[test]
fn reads_hapq_codec() {
    assert_eq!(&read_video_track(&fixture("hapq_64x48.mov")).unwrap().codec, b"HapY");
}

#[test]
fn truncated_file_is_error() {
    let bytes = std::fs::read(fixture("hap1_64x48.mov")).unwrap();
    let p = temp_with(&bytes[..bytes.len() / 2], "trunc.mov");
    assert!(read_video_track(&p).is_err());
}

#[test]
fn garbage_is_error() {
    assert!(read_video_track(&fixture("garbage.mov")).is_err());
}

#[test]
fn missing_file_is_error() {
    assert!(read_video_track(Path::new("C:/definitely/not/here.mov")).is_err());
}
