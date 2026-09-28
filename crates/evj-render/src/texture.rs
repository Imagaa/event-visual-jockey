use crate::Gpu;
use anyhow::{Context, Result};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::IDXGIResource;
use windows::core::Interface;

#[derive(Clone)]
pub struct Texture {
    pub tex: ID3D11Texture2D,
    pub srv: ID3D11ShaderResourceView,
    /// Chroma plane view for NV12 textures (`srv` is then the luma plane).
    pub srv_uv: Option<ID3D11ShaderResourceView>,
    pub width: u32,
    pub height: u32,
    /// Visible part of the (padded) texture in UV space.
    pub uv_scale: [f32; 2],
    /// CPU-writable (Map + WRITE_DISCARD): one copy less per upload than UpdateSubresource.
    pub dynamic: bool,
}

fn tex2d(gpu: &Gpu, format: DXGI_FORMAT, w: u32, h: u32, bind: D3D11_BIND_FLAG) -> Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: w,
        Height: h,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: bind.0 as u32,
        ..Default::default()
    };
    let mut tex = None;
    unsafe { gpu.device.CreateTexture2D(&desc, None, Some(&mut tex))? };
    tex.context("CreateTexture2D returned null")
}

fn srv(gpu: &Gpu, tex: &ID3D11Texture2D) -> Result<ID3D11ShaderResourceView> {
    let mut v = None;
    unsafe { gpu.device.CreateShaderResourceView(tex, None, Some(&mut v))? };
    v.context("CreateShaderResourceView returned null")
}

impl Texture {
    /// Block-compressed texture; dimensions padded to multiples of 4.
    pub fn new_bc(gpu: &Gpu, format: DXGI_FORMAT, w: u32, h: u32) -> Result<Texture> {
        let (pw, ph) = (w.div_ceil(4) * 4, h.div_ceil(4) * 4);
        let tex = tex2d(gpu, format, pw, ph, D3D11_BIND_SHADER_RESOURCE)?;
        Ok(Texture { srv: srv(gpu, &tex)?, srv_uv: None, dynamic: false, tex, width: w, height: h, uv_scale: [w as f32 / pw as f32, h as f32 / ph as f32] })
    }

    /// Block-compressed texture refreshed every frame (HAP): mapped and written directly.
    pub fn new_bc_dynamic(gpu: &Gpu, format: DXGI_FORMAT, w: u32, h: u32) -> Result<Texture> {
        let (pw, ph) = (w.div_ceil(4) * 4, h.div_ceil(4) * 4);
        let desc = D3D11_TEXTURE2D_DESC {
            Width: pw,
            Height: ph,
            MipLevels: 1,
            ArraySize: 1,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DYNAMIC,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
            ..Default::default()
        };
        let mut tex = None;
        unsafe { gpu.device.CreateTexture2D(&desc, None, Some(&mut tex))? };
        let tex = tex.context("CreateTexture2D returned null")?;
        Ok(Texture { srv: srv(gpu, &tex)?, srv_uv: None, dynamic: true, tex, width: w, height: h, uv_scale: [w as f32 / pw as f32, h as f32 / ph as f32] })
    }

    pub fn new_color(gpu: &Gpu, format: DXGI_FORMAT, w: u32, h: u32) -> Result<Texture> {
        let tex = tex2d(gpu, format, w, h, D3D11_BIND_SHADER_RESOURCE)?;
        Ok(Texture { srv: srv(gpu, &tex)?, srv_uv: None, dynamic: false, tex, width: w, height: h, uv_scale: [1.0, 1.0] })
    }

    /// NV12 (hardware video decoder output): luma plane in `srv` (R8), chroma in `srv_uv` (R8G8).
    pub fn new_nv12(gpu: &Gpu, w: u32, h: u32) -> Result<Texture> {
        let tex = tex2d(gpu, DXGI_FORMAT_NV12, w, h, D3D11_BIND_SHADER_RESOURCE)?;
        let plane = |format| -> Result<ID3D11ShaderResourceView> {
            let desc = D3D11_SHADER_RESOURCE_VIEW_DESC {
                Format: format,
                ViewDimension: windows::Win32::Graphics::Direct3D::D3D11_SRV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_SHADER_RESOURCE_VIEW_DESC_0 { Texture2D: D3D11_TEX2D_SRV { MostDetailedMip: 0, MipLevels: 1 } },
            };
            let mut v = None;
            unsafe { gpu.device.CreateShaderResourceView(&tex, Some(&desc), Some(&mut v))? };
            v.context("NV12 plane view")
        };
        Ok(Texture { srv: plane(DXGI_FORMAT_R8_UNORM)?, srv_uv: Some(plane(DXGI_FORMAT_R8G8_UNORM)?), dynamic: false, tex, width: w, height: h, uv_scale: [1.0, 1.0] })
    }

    /// Tight RGBA8 copy (R8G8B8A8 textures only: composition, effect targets).
    pub fn readback(&self, gpu: &Gpu) -> Result<Vec<u8>> {
        RenderTarget::read(gpu, &self.tex, self.width, self.height)
    }

    /// `row_pitch`: bytes per row (BC: bytes per row of 4×4 blocks).
    pub fn upload(&self, ctx: &ID3D11DeviceContext, data: &[u8], row_pitch: u32) {
        if !self.dynamic {
            unsafe { ctx.UpdateSubresource(&self.tex, 0, None, data.as_ptr().cast(), row_pitch, 0) };
            return;
        }
        let pitch = row_pitch as usize;
        let rows = if pitch == 0 { 0 } else { data.len() / pitch };
        unsafe {
            let mut map = D3D11_MAPPED_SUBRESOURCE::default();
            if ctx.Map(&self.tex, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&mut map)).is_err() {
                return; // device lost: the engine rebuilds everything
            }
            let dst = map.pData as *mut u8;
            if map.RowPitch as usize == pitch {
                std::ptr::copy_nonoverlapping(data.as_ptr(), dst, pitch * rows);
            } else {
                for r in 0..rows {
                    std::ptr::copy_nonoverlapping(data.as_ptr().add(r * pitch), dst.add(r * map.RowPitch as usize), pitch);
                }
            }
            ctx.Unmap(&self.tex, 0);
        }
    }
}

pub struct RenderTarget {
    pub tex: ID3D11Texture2D,
    pub rtv: ID3D11RenderTargetView,
    pub srv: ID3D11ShaderResourceView,
    pub width: u32,
    pub height: u32,
}

impl RenderTarget {
    pub fn new(gpu: &Gpu, w: u32, h: u32) -> Result<RenderTarget> {
        let tex = tex2d(gpu, DXGI_FORMAT_R8G8B8A8_UNORM, w, h, D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE)?;
        let mut rtv = None;
        unsafe { gpu.device.CreateRenderTargetView(&tex, None, Some(&mut rtv))? };
        Ok(RenderTarget { rtv: rtv.context("CreateRenderTargetView returned null")?, srv: srv(gpu, &tex)?, tex, width: w, height: h })
    }

    /// The target as a sampleable texture (same GPU resource).
    pub fn as_texture(&self) -> Texture {
        Texture { tex: self.tex.clone(), srv: self.srv.clone(), srv_uv: None, dynamic: false, width: self.width, height: self.height, uv_scale: [1.0, 1.0] }
    }

    /// Render target another D3D11 device can open (`OpenSharedResource`), guarded by a keyed mutex (key 0).
    pub fn new_shared(gpu: &Gpu, w: u32, h: u32) -> Result<(RenderTarget, isize)> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_RENDER_TARGET | D3D11_BIND_SHADER_RESOURCE).0 as u32,
            MiscFlags: D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0 as u32,
            ..Default::default()
        };
        let mut tex = None;
        unsafe { gpu.device.CreateTexture2D(&desc, None, Some(&mut tex))? };
        let tex: ID3D11Texture2D = tex.context("shared texture")?;
        let mut rtv = None;
        unsafe { gpu.device.CreateRenderTargetView(&tex, None, Some(&mut rtv))? };
        let handle = unsafe { tex.cast::<IDXGIResource>()?.GetSharedHandle()? };
        let rt = RenderTarget { rtv: rtv.context("rtv")?, srv: srv(gpu, &tex)?, tex, width: w, height: h };
        Ok((rt, handle.0 as isize))
    }

    /// Tight RGBA8 copy of the target (tests, thumbnails).
    pub fn readback(&self, gpu: &Gpu) -> Result<Vec<u8>> {
        RenderTarget::read(gpu, &self.tex, self.width, self.height)
    }

    fn read(gpu: &Gpu, tex: &ID3D11Texture2D, width: u32, height: u32) -> Result<Vec<u8>> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: width,
            Height: height,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_R8G8B8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut staging = None;
        unsafe { gpu.device.CreateTexture2D(&desc, None, Some(&mut staging))? };
        let staging = staging.context("staging texture")?;
        let mut out = Vec::with_capacity((width * height * 4) as usize);
        unsafe {
            gpu.ctx.CopyResource(&staging, tex);
            let mut map = D3D11_MAPPED_SUBRESOURCE::default();
            gpu.ctx.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut map))?;
            for y in 0..height as usize {
                let row = std::slice::from_raw_parts((map.pData as *const u8).add(y * map.RowPitch as usize), width as usize * 4);
                out.extend_from_slice(row);
            }
            gpu.ctx.Unmap(&staging, 0);
        }
        Ok(out)
    }
}
