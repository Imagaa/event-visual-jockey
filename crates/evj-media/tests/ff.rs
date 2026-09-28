#![cfg(feature = "ffmpeg")]
use evj_media::ff::FfVideo;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

#[test]
fn decodes_prores_and_loops() {
    let mut v = FfVideo::open(&fixture("prores_320x240.mov")).unwrap();
    assert_eq!((v.width, v.height), (320, 240));
    let mut buf = Vec::new();
    let mut n = 0;
    while let Some(_pts) = v.next_frame(&mut buf).unwrap() {
        assert_eq!(buf.len(), 320 * 240 * 4);
        n += 1;
    }
    assert_eq!(n, 5);
    v.rewind().unwrap();
    assert!(v.next_frame(&mut buf).unwrap().is_some());
}

#[test]
fn garbage_is_error() {
    assert!(FfVideo::open(&fixture("garbage.mov")).is_err());
}
