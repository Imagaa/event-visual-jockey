use evj_render::{Compositor, DeviceKind, Gpu, Shade, Texture, fit_rect};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM;

fn solid(gpu: &Gpu, w: u32, h: u32, rgba: [u8; 4]) -> Texture {
    let t = Texture::new_color(gpu, DXGI_FORMAT_R8G8B8A8_UNORM, w, h).unwrap();
    t.upload(&gpu.ctx, &rgba.repeat((w * h) as usize), w * 4);
    t
}

const FULL: [f32; 4] = [0.0, 0.0, 4.0, 4.0];

fn cpu_blend(mode: u32, b: [f64; 3], s: [f64; 3], opacity: f64) -> [f64; 3] {
    let f = |b: f64, s: f64| match mode {
        0 | 9 => s,
        1 => (b + s).min(1.0),
        2 => 1.0 - (1.0 - b) * (1.0 - s),
        3 => b * s,
        4 => if b < 0.5 { 2.0 * b * s } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - s) },
        5 => (b - s).abs(),
        6 => b.max(s),
        7 => b.min(s),
        8 => (b - s).max(0.0),
        _ => unreachable!(),
    };
    let mut a = opacity;
    if mode == 9 {
        a *= 0.2126 * s[0] + 0.7152 * s[1] + 0.0722 * s[2];
    }
    [0, 1, 2].map(|i| b[i] + (f(b[i], s[i]) - b[i]) * a)
}

#[test]
fn all_blend_modes_match_cpu_reference() {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let mut comp = Compositor::new(&gpu, 4, 4).unwrap();
    let (b8, s8) = ([51u8, 102, 153, 255], [178u8, 128, 77, 255]);
    let base = solid(&gpu, 4, 4, b8);
    let layer = solid(&gpu, 4, 4, s8);
    let (b, s) = ([0.2, 0.4, 0.6], [178.0 / 255.0, 128.0 / 255.0, 77.0 / 255.0]);
    for mode in 0..10u32 {
        for opacity in [1.0f32, 0.5] {
            comp.begin(&gpu.ctx);
            comp.layer(&gpu.ctx, &base, Shade::Rgba, FULL, 0, 1.0);
            comp.layer(&gpu.ctx, &layer, Shade::Rgba, FULL, mode, opacity);
            let px = comp.output().readback(&gpu).unwrap();
            let want = cpu_blend(mode, b, s, opacity as f64).map(|v| (v * 255.0).round() as i32);
            for p in px.chunks(4) {
                for c in 0..3 {
                    assert!((p[c] as i32 - want[c]).abs() <= 2, "mode {mode} opacity {opacity}: {p:?} vs {want:?}");
                }
            }
        }
    }
}

#[test]
fn fixed_blends_match_the_shader_blend() {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let mut comp = Compositor::new(&gpu, 4, 4).unwrap();
    let base = solid(&gpu, 4, 4, [51, 102, 153, 255]);
    for s8 in [[178u8, 128, 77, 255], [178, 128, 77, 128]] {
        let layer = solid(&gpu, 4, 4, s8);
        for mode in [0u32, 2, 3] {
            for opacity in [1.0f32, 0.5] {
                comp.begin(&gpu.ctx);
                comp.layer(&gpu.ctx, &base, Shade::Rgba, FULL, 0, 1.0);
                comp.layer(&gpu.ctx, &layer, Shade::Rgba, FULL, mode, opacity);
                let want = comp.output().readback(&gpu).unwrap();
                comp.begin(&gpu.ctx);
                comp.layer(&gpu.ctx, &base, Shade::Rgba, FULL, 0, 1.0);
                assert!(comp.blit_fixed(&gpu.ctx, &layer, Shade::Rgba, FULL, mode, opacity), "mode {mode} is fixed-function");
                let got = comp.output().readback(&gpu).unwrap();
                for (g, w) in got.iter().zip(&want) {
                    assert!((*g as i32 - *w as i32).abs() <= 2, "mode {mode} opacity {opacity} alpha {}: {:?} vs {:?}", s8[3], &got[..4], &want[..4]);
                }
            }
        }
    }
    assert!(!comp.blit_fixed(&gpu.ctx, &base, Shade::Rgba, FULL, 4, 1.0), "overlay needs the shader blend");
}

#[test]
fn fixed_blit_keeps_letterbox_bars_untouched() {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let mut comp = Compositor::new(&gpu, 8, 4).unwrap();
    let red = solid(&gpu, 2, 4, [255, 0, 0, 255]);
    comp.begin(&gpu.ctx);
    assert!(comp.blit_fixed(&gpu.ctx, &red, Shade::Rgba, fit_rect(2, 4, 8, 4, 0), 0, 1.0));
    let px = comp.output().readback(&gpu).unwrap();
    let at = |x: usize, y: usize| &px[(y * 8 + x) * 4..(y * 8 + x) * 4 + 3];
    assert_eq!(at(0, 1), [0, 0, 0]);
    assert_eq!(at(3, 1), [255, 0, 0]);
    assert_eq!(at(7, 2), [0, 0, 0]);
}

#[test]
fn fit_rect_modes() {
    // 1:2 source into 2:1 target
    assert_eq!(fit_rect(2, 4, 8, 4, 0), [3.0, 0.0, 2.0, 4.0]);
    assert_eq!(fit_rect(2, 4, 8, 4, 1), [0.0, -6.0, 8.0, 16.0]);
    assert_eq!(fit_rect(2, 4, 8, 4, 2), [0.0, 0.0, 8.0, 4.0]);
}

#[test]
fn fit_leaves_transparent_bars_fill_covers() {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let mut comp = Compositor::new(&gpu, 8, 4).unwrap();
    let red = solid(&gpu, 2, 4, [255, 0, 0, 255]);
    comp.begin(&gpu.ctx);
    comp.layer(&gpu.ctx, &red, Shade::Rgba, fit_rect(2, 4, 8, 4, 0), 0, 1.0);
    let px = comp.output().readback(&gpu).unwrap();
    let at = |x: usize, y: usize| &px[(y * 8 + x) * 4..(y * 8 + x) * 4 + 3];
    assert_eq!(at(0, 1), [0, 0, 0]);
    assert_eq!(at(3, 1), [255, 0, 0]);
    assert_eq!(at(4, 2), [255, 0, 0]);
    assert_eq!(at(7, 2), [0, 0, 0]);

    comp.begin(&gpu.ctx);
    comp.layer(&gpu.ctx, &red, Shade::Rgba, fit_rect(2, 4, 8, 4, 1), 0, 1.0);
    let px = comp.output().readback(&gpu).unwrap();
    assert!(px.chunks(4).all(|p| p[..3] == [255, 0, 0]));
}

#[test]
fn begin_clears_to_black() {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let mut comp = Compositor::new(&gpu, 4, 4).unwrap();
    comp.begin(&gpu.ctx);
    let px = comp.output().readback(&gpu).unwrap();
    assert!(px.chunks(4).all(|p| p == [0, 0, 0, 255]));
}
