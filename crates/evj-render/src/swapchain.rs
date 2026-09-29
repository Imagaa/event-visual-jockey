use anyhow::{Context, Result};
use crate::Gpu;
use windows::Win32::Foundation::{DXGI_STATUS_OCCLUDED, HANDLE, HWND};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::Win32::System::Threading::WaitForSingleObjectEx;
use windows::core::Interface;

const FLAGS: DXGI_SWAP_CHAIN_FLAG = DXGI_SWAP_CHAIN_FLAG_FRAME_LATENCY_WAITABLE_OBJECT;

/// Flip-model swapchain with a frame-latency waitable (lowest-latency pacing).
pub struct Swapchain {
    chain: IDXGISwapChain2,
    waitable: HANDLE,
    rtv: Option<ID3D11RenderTargetView>,
}

impl Swapchain {
    pub fn new(gpu: &Gpu, hwnd: HWND, w: u32, h: u32) -> Result<Swapchain> {
        unsafe {
            let factory: IDXGIFactory2 = gpu.device.cast::<IDXGIDevice>()?.GetAdapter()?.GetParent()?;
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                Width: w.max(1),
                Height: h.max(1),
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                Flags: FLAGS.0 as u32,
                ..Default::default()
            };
            let chain: IDXGISwapChain2 = factory.CreateSwapChainForHwnd(&gpu.device, hwnd, &desc, None, None)?.cast()?;
            factory.MakeWindowAssociation(hwnd, DXGI_MWA_NO_ALT_ENTER)?;
            chain.SetMaximumFrameLatency(1)?;
            let waitable = chain.GetFrameLatencyWaitableObject();
            let mut s = Swapchain { chain, waitable, rtv: None };
            s.create_rtv(gpu)?;
            Ok(s)
        }
    }

    fn create_rtv(&mut self, gpu: &Gpu) -> Result<()> {
        unsafe {
            let back: ID3D11Texture2D = self.chain.GetBuffer(0)?;
            let mut rtv = None;
            gpu.device.CreateRenderTargetView(&back, None, Some(&mut rtv))?;
            self.rtv = Some(rtv.context("rtv")?);
        }
        Ok(())
    }

    /// How many frames may wait for the screen. 1 = lowest latency (outputs); a window that
    /// DWM composites needs 2, or every frame waits two vblanks (30 fps on a 60 Hz screen).
    pub fn set_max_latency(&self, frames: u32) -> Result<()> {
        unsafe { self.chain.SetMaximumFrameLatency(frames)? };
        Ok(())
    }

    /// Blocks until the swapchain can take a new frame.
    pub fn wait(&self) {
        unsafe { WaitForSingleObjectEx(self.waitable, 1000, true) };
    }

    pub fn rtv(&self) -> Option<&ID3D11RenderTargetView> {
        self.rtv.as_ref()
    }

    /// Minimised windows report 0×0: keep the old buffers.
    pub fn resize(&mut self, gpu: &Gpu, w: u32, h: u32) -> Result<()> {
        if w == 0 || h == 0 {
            return Ok(());
        }
        self.rtv = None;
        unsafe { self.chain.ResizeBuffers(0, w, h, DXGI_FORMAT_UNKNOWN, FLAGS)? };
        self.create_rtv(gpu)
    }

    /// Presents with vsync; false when the window is minimised or covered (nothing was shown,
    /// and the next `wait` will not pace the caller).
    pub fn present(&self) -> Result<bool> {
        self.present_with(1)
    }

    /// `sync_interval` 0 = do not wait for vblank (secondary outputs). Returns whether the window is visible.
    pub fn present_with(&self, sync_interval: u32) -> Result<bool> {
        let hr = unsafe { self.chain.Present(sync_interval, DXGI_PRESENT(0)) };
        hr.ok()?;
        Ok(hr != DXGI_STATUS_OCCLUDED)
    }
}
