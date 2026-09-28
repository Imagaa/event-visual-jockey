use evj_media::hap::{HapFormat, HapReader, decode_frame, texture_len};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn frames(name: &str) -> (HapFormat, Vec<Vec<u8>>) {
    let mut r = HapReader::open(&fixture(name)).unwrap();
    let mut fmt = None;
    let out = (0..r.track.samples.len())
        .map(|i| {
            let mut buf = Vec::new();
            fmt = Some(r.read_frame(i, &mut buf).unwrap());
            buf
        })
        .collect();
    (fmt.unwrap(), out)
}

#[test]
fn hap1_decodes_to_bc1_of_expected_size() {
    let (fmt, f) = frames("hap1_64x48.mov");
    assert_eq!(fmt, HapFormat::Bc1);
    assert_eq!(f.len(), 3);
    assert!(f.iter().all(|b| b.len() == 16 * 12 * 8));
}

#[test]
fn chunked_and_uncompressed_match_snappy() {
    let (_, a) = frames("hap1_64x48.mov");
    let (_, b) = frames("hap1_64x48_chunked.mov");
    let (_, c) = frames("hap1_64x48_raw.mov");
    assert_eq!(a, b);
    assert_eq!(a, c);
}

#[test]
fn hap_alpha_and_q_formats() {
    assert_eq!(frames("hap5_64x48.mov").0, HapFormat::Bc3);
    let (fmt, f) = frames("hapq_64x48.mov");
    assert_eq!(fmt, HapFormat::YCoCgBc3);
    assert!(f.iter().all(|b| b.len() == 16 * 12 * 16));
}

#[test]
fn padded_sizes() {
    assert_eq!(texture_len(HapFormat::Bc1, 66, 50), 17 * 13 * 8);
    assert_eq!(texture_len(HapFormat::Bc3, 1920, 1080), 480 * 270 * 16);
}

#[test]
fn unsupported_format_is_error() {
    // 4-byte header: size 4, type 0xA1 = uncompressed BC7 (HAP R)
    let err = decode_frame(&[4, 0, 0, 0xA1, 1, 2, 3, 4], &mut Vec::new()).unwrap_err();
    assert!(err.to_string().contains("unsupported"), "{err}");
}

#[test]
fn truncated_and_garbage_are_errors() {
    assert!(decode_frame(&[], &mut Vec::new()).is_err());
    assert!(decode_frame(&[0, 0, 0, 0xBB, 0xFF, 0xFF, 0, 0], &mut Vec::new()).is_err());
    assert!(decode_frame(&[8, 0, 0, 0xBB, 1, 2, 3, 4, 5, 6, 7, 8], &mut Vec::new()).is_err());
}

#[test]
fn non_hap_file_is_rejected() {
    assert!(HapReader::open(&fixture("prores_320x240.mov")).is_err());
}
