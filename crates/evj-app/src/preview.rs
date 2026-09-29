//! An engine picture (Program / Preview monitor, transition card) opened on a UI device: the
//! engine's ring of shared textures (keyed mutex each). Each UI frame draws the slot the engine
//! finished a frame ago, so drawing it never waits for the engine's GPU work.
use anyhow::{Context, Result};
use evj_engine::SharedPreview;
use evj_render::Gpu;
use std::cell::Cell;
use std::sync::atomic::Ordering;
use windows::Win32::Foundation::{HANDLE, S_OK};
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::core::Interface;

struct Slot {
    tex_id: egui::TextureId,
    mutex: IDXGIKeyedMutex,
    _tex: ID3D11Texture2D,
}

pub struct Preview {
    pub size: [f32; 2],
    /// The engine's ring (another window — the presenter — opens the same one).
    pub shared: SharedPreview,
    slots: Vec<Slot>,
    /// The slot this UI frame draws (picked by `tex_id`, held by `lock`).
    slot: Cell<usize>,
}

impl Preview {
    pub fn open(gpu: &Gpu, renderer: &mut egui_directx11::Renderer, shared: &SharedPreview) -> Result<Preview> {
        let mut slots = Vec::new();
        for &handle in &shared.handles {
            unsafe {
                let mut tex: Option<ID3D11Texture2D> = None;
                gpu.device.OpenSharedResource(HANDLE(handle as _), &mut tex)?;
                let tex = tex.context("OpenSharedResource returned null")?;
                let mut srv = None;
                gpu.device.CreateShaderResourceView(&tex, None, Some(&mut srv))?;
                let tex_id = renderer.register_user_texture(srv.context("preview srv")?);
                let mutex = tex.cast::<IDXGIKeyedMutex>()?;
                slots.push(Slot { tex_id, mutex, _tex: tex });
            }
        }
        anyhow::ensure!(!slots.is_empty(), "the engine shared no preview texture");
        Ok(Preview { size: [shared.width as f32, shared.height as f32], shared: shared.clone(), slots, slot: Cell::new(0) })
    }

    /// The texture to draw this frame: the newest picture the engine has finished.
    pub fn tex_id(&self) -> egui::TextureId {
        let i = self.shared.ready.load(Ordering::Acquire).min(self.slots.len() - 1);
        self.slot.set(i);
        self.slots[i].tex_id
    }

    /// Holds this frame's texture while the UI draws it; false if the engine is writing it.
    pub fn lock(&self) -> bool {
        let m = &self.slots[self.slot.get()].mutex;
        unsafe { (Interface::vtable(m).AcquireSync)(Interface::as_raw(m), 0, 4) == S_OK }
    }

    pub fn unlock(&self) {
        unsafe {
            let _ = self.slots[self.slot.get()].mutex.ReleaseSync(0);
        }
    }

    /// Frees the renderer's handles to the textures (before opening a new ring).
    pub fn release(self, renderer: &mut egui_directx11::Renderer) {
        for s in self.slots {
            renderer.unregister_user_texture(s.tex_id);
        }
    }
}
