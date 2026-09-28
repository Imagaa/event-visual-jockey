use evj_media::image::load_image;
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

#[test]
fn loads_png_as_bgra() {
    let (w, h, px) = load_image(&fixture("red_64x36.png"), 4096).unwrap();
    assert_eq!((w, h), (64, 36));
    assert_eq!(px.len(), 64 * 36 * 4);
    // BGRA red (ffmpeg writes 253 after its YUV round trip)
    assert!(px[0] < 5 && px[1] < 5 && px[2] > 250 && px[3] == 255, "{:?}", &px[..4]);
}

#[test]
fn loads_jpg() {
    let (w, h, px) = load_image(&fixture("blue_36x64.jpg"), 4096).unwrap();
    assert_eq!((w, h), (36, 64));
    assert!(px[0] > 200 && px[2] < 50, "{:?}", &px[..4]); // blue-ish
}

/// Inserts an EXIF APP1 segment with the given orientation right after the JPEG SOI marker.
fn with_orientation(jpg: &[u8], orientation: u8) -> Vec<u8> {
    let mut app1 = vec![0xFF, 0xE1, 0x00, 0x22];
    app1.extend_from_slice(b"Exif\0\0II\x2A\0\x08\0\0\0");
    app1.extend_from_slice(&[1, 0, 0x12, 0x01, 3, 0, 1, 0, 0, 0, orientation, 0, 0, 0, 0, 0, 0, 0]);
    let mut out = jpg[..2].to_vec();
    out.extend_from_slice(&app1);
    out.extend_from_slice(&jpg[2..]);
    out
}

#[test]
fn exif_rotation_is_applied() {
    let jpg = std::fs::read(fixture("blue_36x64.jpg")).unwrap();
    let p = std::env::temp_dir().join(format!("evj-rot-{}.jpg", std::process::id()));
    std::fs::write(&p, with_orientation(&jpg, 6)).unwrap();
    let (w, h, _) = load_image(&p, 4096).unwrap();
    assert_eq!((w, h), (64, 36), "orientation 6 = rotate 90°");
}

#[test]
fn huge_images_are_downscaled() {
    let (w, h, px) = load_image(&fixture("red_64x36.png"), 32).unwrap();
    assert_eq!((w, h), (32, 18));
    assert_eq!(px.len(), 32 * 18 * 4);
}

#[test]
fn bad_images_are_errors() {
    assert!(load_image(&fixture("garbage.mov"), 4096).is_err());
    assert!(load_image(Path::new("C:/nope/x.png"), 4096).is_err());
}
