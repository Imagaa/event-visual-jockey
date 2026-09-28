use anyhow::{Context, Result};
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_DRIVER_TYPE_WARP, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::*;
use windows::core::Interface;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceKind {
    Hardware,
    /// Software rasterizer — headless tests.
    Warp,
}

pub struct Gpu {
    pub device: ID3D11Device,
    pub ctx: ID3D11DeviceContext,
    pub adapter_name: String,
}

/// Which GPU drives each display: (Windows display name such as `\\.\DISPLAY2`, adapter name).
/// On hybrid laptops a screen wired to the other GPU gets its frames copied across (slower).
pub fn display_adapters() -> Vec<(String, String)> {
    let mut out = Vec::new();
    unsafe {
        let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() else { return out };
        for i in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(i) else { break };
            let Ok(desc) = adapter.GetDesc1() else { continue };
            let gpu = String::from_utf16_lossy(&desc.Description).trim_end_matches('\0').to_string();
            for j in 0.. {
                let Ok(output) = adapter.EnumOutputs(j) else { break };
                if let Ok(d) = output.GetDesc() {
                    out.push((String::from_utf16_lossy(&d.DeviceName).trim_end_matches('\0').to_string(), gpu.clone()));
                }
            }
        }
    }
    out
}

impl Gpu {
    pub fn new(kind: DeviceKind) -> Result<Gpu> {
        let (mut device, mut ctx) = (None, None);
        let flags = D3D11_CREATE_DEVICE_BGRA_SUPPORT;
        unsafe {
            match kind {
                DeviceKind::Hardware => {
                    let factory: IDXGIFactory6 = CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS(0))?;
                    let adapter: IDXGIAdapter1 = factory.EnumAdapterByGpuPreference(0, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE)?;
                    D3D11CreateDevice(
                        &adapter,
                        D3D_DRIVER_TYPE_UNKNOWN,
                        HMODULE::default(),
                        flags | D3D11_CREATE_DEVICE_VIDEO_SUPPORT,
                        Some(&[D3D_FEATURE_LEVEL_11_0]),
                        D3D11_SDK_VERSION,
                        Some(&mut device),
                        None,
                        Some(&mut ctx),
                    )?;
                }
                DeviceKind::Warp => {
                    D3D11CreateDevice(
                        None,
                        D3D_DRIVER_TYPE_WARP,
                        HMODULE::default(),
                        flags,
                        Some(&[D3D_FEATURE_LEVEL_11_0]),
                        D3D11_SDK_VERSION,
                        Some(&mut device),
                        None,
                        Some(&mut ctx),
                    )?;
                }
            }
        }
        let device: ID3D11Device = device.context("D3D11CreateDevice returned no device")?;
        let ctx = ctx.context("D3D11CreateDevice returned no context")?;
        // Decoder threads (Media Foundation) share this device.
        unsafe {
            let _ = device.cast::<ID3D11Multithread>()?.SetMultithreadProtected(true);
        }
        let adapter_name = unsafe {
            let desc = device.cast::<IDXGIDevice>()?.GetAdapter()?.GetDesc()?;
            String::from_utf16_lossy(&desc.Description).trim_end_matches('\0').to_string()
        };
        Ok(Gpu { device, ctx, adapter_name })
    }

    /// Video memory this process uses (MB): dedicated + shared (iGPUs mostly use shared).
    pub fn memory_mb(&self) -> Option<f32> {
        unsafe {
            let adapter: IDXGIAdapter3 = self.device.cast::<IDXGIDevice>().ok()?.GetAdapter().ok()?.cast().ok()?;
            let mut total = 0u64;
            for group in [DXGI_MEMORY_SEGMENT_GROUP_LOCAL, DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL] {
                let mut info = DXGI_QUERY_VIDEO_MEMORY_INFO::default();
                adapter.QueryVideoMemoryInfo(0, group, &mut info).ok()?;
                total += info.CurrentUsage;
            }
            Some(total as f32 / (1024.0 * 1024.0))
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_display_names_its_gpu() {
        for (display, gpu) in super::display_adapters() {
            assert!(display.starts_with(r"\\.\"), "{display}");
            assert!(!gpu.is_empty());
        }
    }
}
