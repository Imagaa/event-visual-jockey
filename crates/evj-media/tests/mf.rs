use evj_media::mf::{MfVideo, VideoFrame, mf_init_thread};
use evj_render::{DeviceKind, Gpu};
use std::path::{Path, PathBuf};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name)
}

fn count_frames(v: &mut MfVideo) -> (usize, usize) {
    let (mut gpu, mut cpu) = (0, 0);
    while let Some((f, _)) = v.next_frame().unwrap() {
        match f {
            VideoFrame::Gpu { .. } => gpu += 1,
            VideoFrame::Cpu { data } => {
                assert_eq!(data.len(), 320 * 240 * 4);
                assert!(data.iter().any(|&b| b != 0), "blank frame");
                cpu += 1;
            }
        }
    }
    (gpu, cpu)
}

#[test]
fn decodes_h264_to_system_memory_and_rewinds() {
    mf_init_thread().unwrap();
    let mut v = MfVideo::open(&fixture("h264_320x240.mp4"), None).unwrap();
    assert_eq!((v.width, v.height), (320, 240));
    assert!((v.fps - 30.0).abs() < 0.1, "fps {}", v.fps);
    assert_eq!(count_frames(&mut v), (0, 30));
    v.rewind().unwrap();
    assert_eq!(count_frames(&mut v).1, 30);
}

#[test]
fn decodes_h264_with_hardware_device() {
    mf_init_thread().unwrap();
    let gpu = Gpu::new(DeviceKind::Hardware).unwrap();
    let mut v = MfVideo::open(&fixture("h264_320x240.mp4"), Some(&gpu.device)).unwrap();
    let (g, c) = count_frames(&mut v);
    assert_eq!(g + c, 30, "gpu {g} cpu {c}");
    eprintln!("hardware path: gpu frames {g}, cpu frames {c}");
}

/// Row 2 of frame 0 as BGRA, produced by ffmpeg (tools/make-fixtures.sh).
fn reference_row(y: usize) -> Vec<u8> {
    let raw = std::fs::read(fixture("h264_320x240_f0.bgra")).unwrap();
    raw[y * 320 * 4..(y + 1) * 320 * 4].to_vec()
}

#[test]
fn cpu_frame_is_top_down() {
    mf_init_thread().unwrap();
    let mut v = MfVideo::open(&fixture("h264_320x240.mp4"), None).unwrap();
    let Some((VideoFrame::Cpu { data }, _)) = v.next_frame().unwrap() else { panic!("expected cpu frame") };
    let diff = |y: usize| -> i64 {
        let row = &data[y * 320 * 4..(y + 1) * 320 * 4];
        row.iter().zip(reference_row(y)).step_by(4).map(|(a, b)| (*a as i64 - b as i64).abs()).sum::<i64>() / 320
    };
    // Same rows should match closely; a vertically flipped image would not.
    assert!(diff(2) < 30 && diff(200) < 30, "rows differ: top {} bottom {}", diff(2), diff(200));
}

#[test]
fn garbage_is_error() {
    mf_init_thread().unwrap();
    assert!(MfVideo::open(&fixture("garbage.mov"), None).is_err());
}

#[test]
fn hardware_frames_are_nv12_with_colour_matrix() {
    use windows::Win32::Graphics::Direct3D11::D3D11_TEXTURE2D_DESC;
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_NV12;
    mf_init_thread().unwrap();
    let gpu = Gpu::new(DeviceKind::Hardware).unwrap();
    let mut v = MfVideo::open(&fixture("h264_320x240.mp4"), Some(&gpu.device)).unwrap();
    assert!(!v.bt709, "SD without a matrix tag defaults to BT.601");
    let Some((VideoFrame::Gpu { texture, .. }, _)) = v.next_frame().unwrap() else { panic!("expected gpu frame") };
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe { texture.GetDesc(&mut desc) };
    assert_eq!(desc.Format, DXGI_FORMAT_NV12);
}

#[test]
fn seek_lands_near_target() {
    mf_init_thread().unwrap();
    let mut v = MfVideo::open(&fixture("h264_320x240.mp4"), None).unwrap();
    v.seek(0.5).unwrap();
    let (_, pts) = v.next_frame().unwrap().unwrap();
    assert!(pts >= 0.5 - 1.0 / 30.0 - 1e-6, "pts {pts}");
    assert!(pts < 0.9, "pts {pts}");
}
