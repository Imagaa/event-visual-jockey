use evj_render::{Blitter, DeviceKind, Gpu, RenderTarget, Shade, Texture};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_FORMAT_BC1_UNORM, DXGI_FORMAT_BC3_UNORM};

fn blit_one_block(format: DXGI_FORMAT, block: &[u8], shade: Shade) -> Vec<u8> {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let tex = Texture::new_bc(&gpu, format, 4, 4).unwrap();
    tex.upload(&gpu.ctx, block, block.len() as u32);
    let rt = RenderTarget::new(&gpu, 4, 4).unwrap();
    Blitter::new(&gpu).unwrap().draw(&gpu.ctx, &tex, shade, &rt.rtv, [0.0, 0.0, 4.0, 4.0]);
    rt.readback(&gpu).unwrap()
}

#[test]
fn bc1_red_block_renders_red() {
    // color0 = 0xF800 (red), color1 = 0, all indices 0
    let px = blit_one_block(DXGI_FORMAT_BC1_UNORM, &[0x00, 0xF8, 0, 0, 0, 0, 0, 0], Shade::Rgba);
    assert!(px.chunks(4).all(|p| p == [255, 0, 0, 255]), "{px:?}");
}

#[test]
fn ycocg_block_converts_to_rgb() {
    // alpha (=Y) block: a0=200, a1=0, indices 0 → Y = 200/255
    // color block: color0 = R 16/31 (Co), G 32/63 (Cg), B 0 (scale) ; indices 0
    let c0: u16 = (16 << 11) | (32 << 5);
    let mut block = vec![200, 0, 0, 0, 0, 0, 0, 0];
    block.extend_from_slice(&c0.to_le_bytes());
    block.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    let px = blit_one_block(DXGI_FORMAT_BC3_UNORM, &block, Shade::YCoCg);

    let off = 128.0 / 255.0;
    let (co, cg, y) = (16.0 / 31.0 - off, 32.0 / 63.0 - off, 200.0 / 255.0);
    let want = [y + co - cg, y + cg, y - co - cg].map(|v: f64| (v * 255.0).round() as i32);
    for p in px.chunks(4) {
        for c in 0..3 {
            assert!((p[c] as i32 - want[c]).abs() <= 3, "{p:?} vs {want:?}");
        }
        assert_eq!(p[3], 255);
    }
}

#[test]
fn padded_bc_texture_reports_visible_uv() {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let t = Texture::new_bc(&gpu, DXGI_FORMAT_BC1_UNORM, 66, 50).unwrap();
    assert_eq!((t.width, t.height), (66, 50));
    assert!((t.uv_scale[0] - 66.0 / 68.0).abs() < 1e-6 && (t.uv_scale[1] - 50.0 / 52.0).abs() < 1e-6);
}

fn nv12_pixel(bt709: bool) -> Vec<u8> {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let tex = Texture::new_nv12(&gpu, 4, 4).unwrap();
    // 4x4 luma rows (Y=180) followed by 2 rows of interleaved chroma (Cb=90, Cr=200), row pitch 4.
    let mut data = vec![180u8; 16];
    data.extend_from_slice(&[90, 200, 90, 200, 90, 200, 90, 200]);
    tex.upload(&gpu.ctx, &data, 4);
    let rt = RenderTarget::new(&gpu, 4, 4).unwrap();
    Blitter::new(&gpu).unwrap().draw(&gpu.ctx, &tex, Shade::Nv12 { bt709 }, &rt.rtv, [0.0, 0.0, 4.0, 4.0]);
    rt.readback(&gpu).unwrap()
}

fn expect_yuv(px: &[u8], (kr_cr, kg_cb, kg_cr, kb_cb): (f64, f64, f64, f64)) {
    // limited range: Y 16..235, C 16..240
    let (y, cb, cr) = ((180.0 - 16.0) / 219.0, (90.0 - 128.0) / 224.0, (200.0 - 128.0) / 224.0);
    let want = [y + kr_cr * cr, y - kg_cb * cb - kg_cr * cr, y + kb_cb * cb].map(|v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as i32);
    for p in px.chunks(4) {
        for c in 0..3 {
            assert!((p[c] as i32 - want[c]).abs() <= 2, "{p:?} vs {want:?}");
        }
        assert_eq!(p[3], 255);
    }
}

#[test]
fn nv12_bt709_limited_range() {
    expect_yuv(&nv12_pixel(true), (1.5748, 0.1873, 0.4681, 1.8556));
}

#[test]
fn nv12_bt601_limited_range() {
    expect_yuv(&nv12_pixel(false), (1.402, 0.344136, 0.714136, 1.772));
}

#[test]
fn draw_region_crops_the_source() {
    use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM;
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    // 4x1 source: red, green, blue, white
    let tex = Texture::new_color(&gpu, DXGI_FORMAT_R8G8B8A8_UNORM, 4, 1).unwrap();
    tex.upload(&gpu.ctx, &[255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255], 16);
    let rt = RenderTarget::new(&gpu, 4, 2).unwrap();
    let b = Blitter::new(&gpu).unwrap();
    // right half of the source (blue, white) stretched over the whole target
    b.draw_region(&gpu.ctx, &tex, Shade::Rgba, &rt.rtv, [0.0, 0.0, 4.0, 2.0], [0.5, 0.0, 0.5, 1.0]);
    let px = rt.readback(&gpu).unwrap();
    let at = |x: usize| &px[x * 4..x * 4 + 3];
    assert!(at(0)[2] > 150 && at(0)[0] < 60, "left = blue {:?}", at(0));
    assert!(at(3).iter().all(|&c| c > 200), "right = white {:?}", at(3));
}

#[test]
fn dynamic_bc_texture_uploads_like_default() {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    // 8x8 = 2x2 blocks: red, green / blue, black
    let block = |c: u16| {
        let mut b = c.to_le_bytes().to_vec();
        b.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
        b
    };
    let data: Vec<u8> = [0xF800u16, 0x07E0, 0x001F, 0x0000].iter().flat_map(|&c| block(c)).collect();
    let tex = Texture::new_bc_dynamic(&gpu, DXGI_FORMAT_BC1_UNORM, 8, 8).unwrap();
    tex.upload(&gpu.ctx, &data, 16);
    let rt = RenderTarget::new(&gpu, 8, 8).unwrap();
    Blitter::new(&gpu).unwrap().draw(&gpu.ctx, &tex, Shade::Rgba, &rt.rtv, [0.0, 0.0, 8.0, 8.0]);
    let px = rt.readback(&gpu).unwrap();
    let at = |x: usize, y: usize| px[(y * 8 + x) * 4..(y * 8 + x) * 4 + 3].to_vec();
    assert_eq!(at(1, 1), vec![255, 0, 0]);
    assert_eq!(at(6, 1), vec![0, 255, 0]);
    assert_eq!(at(1, 6), vec![0, 0, 255]);
}
