//! The engine's composition preview, opened on the UI device (shared texture + keyed mutex).
use anyhow::{Context, Result};
use evj_render::Gpu;
use windows::Win32::Foundation::{HANDLE, S_OK};
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::core::Interface;

pub struct Preview {
    pub tex_id: egui::TextureId,
    pub size: [f32; 2],
    /// The engine's share handle (another window can open the same texture).
    pub handle: isize,
    mutex: IDXGIKeyedMutex,
    _tex: ID3D11Texture2D,
}

impl Preview {
    pub fn open(gpu: &Gpu, renderer: &mut egui_directx11::Renderer, handle: isize, w: u32, h: u32) -> Result<Preview> {
        unsafe {
            let mut tex: Option<ID3D11Texture2D> = None;
            gpu.device.OpenSharedResource(HANDLE(handle as _), &mut tex)?;
            let tex = tex.context("OpenSharedResource returned null")?;
            let mut srv = None;
            gpu.device.CreateShaderResourceView(&tex, None, Some(&mut srv))?;
            let tex_id = renderer.register_user_texture(srv.context("preview srv")?);
            let mutex = tex.cast::<IDXGIKeyedMutex>()?;
            Ok(Preview { tex_id, size: [w as f32, h as f32], handle, mutex, _tex: tex })
        }
    }

    /// Holds the texture while the UI draws it; false if the engine is writing it right now.
    pub fn lock(&self) -> bool {
        unsafe { (Interface::vtable(&self.mutex).AcquireSync)(Interface::as_raw(&self.mutex), 0, 4) == S_OK }
    }

    pub fn unlock(&self) {
        unsafe {
            let _ = self.mutex.ReleaseSync(0);
        }
    }
}
