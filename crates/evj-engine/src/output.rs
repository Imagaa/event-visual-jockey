use anyhow::Result;
use evj_core::output::{OutputConfig, Slice};
use evj_render::{Blitter, Gpu, Shade, Swapchain, Texture};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::ID3D11RenderTargetView;

/// A window the composition is shown on (projector, LED processor, ...).
pub struct Output {
    pub id: u32,
    pub hwnd: isize,
    pub config: OutputConfig,
    chain: Swapchain,
    width: u32,
    height: u32,
    /// The last present reached the screen (not minimised / covered).
    visible: std::cell::Cell<bool>,
}

/// Clears `rtv` and draws every slice of `src` into it.
pub fn draw_slices(gpu: &Gpu, blitter: &Blitter, src: &Texture, slices: &[Slice], rtv: &ID3D11RenderTargetView, width: u32, height: u32) {
    unsafe { gpu.ctx.ClearRenderTargetView(rtv, &[0.0, 0.0, 0.0, 1.0]) };
    let (w, h) = (width as f32, height as f32);
    for s in slices {
        let o = s.output;
        blitter.draw_region(&gpu.ctx, src, Shade::Rgba, rtv, [o[0] * w, o[1] * h, o[2] * w, o[3] * h], s.input);
    }
}

impl Output {
    pub fn new(gpu: &Gpu, id: u32, hwnd: isize, width: u32, height: u32) -> Result<Output> {
        let chain = Swapchain::new(gpu, HWND(hwnd as _), width, height)?;
        Ok(Output { id, hwnd, config: OutputConfig::default(), chain, width, height, visible: std::cell::Cell::new(true) })
    }

    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn resize(&mut self, gpu: &Gpu, width: u32, height: u32) -> Result<()> {
        self.chain.resize(gpu, width, height)?;
        if width > 0 && height > 0 {
            (self.width, self.height) = (width, height);
        }
        Ok(())
    }

    /// Waits for the next vblank; false (at once) when the window is hidden and cannot pace the engine.
    pub fn wait(&self) -> bool {
        if !self.visible.get() {
            return false;
        }
        self.chain.wait();
        true
    }

    /// Draws the slices of `src` and presents (vsync only on the primary output).
    pub fn present(&self, gpu: &Gpu, blitter: &Blitter, src: &Texture, vsync: bool) -> Result<()> {
        let Some(rtv) = self.chain.rtv() else { return Ok(()) };
        draw_slices(gpu, blitter, src, &self.config.slices, rtv, self.width, self.height);
        self.visible.set(self.chain.present_with(if vsync { 1 } else { 0 })?);
        Ok(())
    }
}
