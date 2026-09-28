//! Runs effect / transition pixel shaders (built from `evj_core::effect::build_shader`).
use crate::{Gpu, RenderTarget, Texture, compile};
use anyhow::{Context, Result};
use windows::Win32::Graphics::Direct3D::D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D11::*;

const VS: &str = r#"
struct V { float4 pos : SV_Position; float2 uv : TEXCOORD0; };
V vs(uint id : SV_VertexID) {
    V o; float2 t = float2((id << 1) & 2, id & 2);
    o.pos = float4(t * float2(2, -2) + float2(-1, 1), 0, 1);
    o.uv = t; return o;
}
"#;

pub struct FxProgram {
    ps: ID3D11PixelShader,
}

impl FxProgram {
    /// Compiles a full effect source; the error text includes the file's line numbers.
    pub fn compile(gpu: &Gpu, hlsl: &str) -> Result<FxProgram> {
        let mut ps = None;
        unsafe { gpu.device.CreatePixelShader(&compile(hlsl, "evj_main", "ps_5_0")?, None, Some(&mut ps))? };
        Ok(FxProgram { ps: ps.context("pixel shader")? })
    }
}

#[derive(Clone, Copy)]
pub struct FxParams {
    pub time: f32,
    pub beat: f32,
    /// Transitions: 0 = all source, 1 = all destination.
    pub progress: f32,
    pub values: [f32; 16],
}

/// Ping-pongs between two targets so a chain of passes never reads what it writes.
pub struct FxRunner {
    pub width: u32,
    pub height: u32,
    targets: [RenderTarget; 2],
    next: usize,
    vs: ID3D11VertexShader,
    cbuf: ID3D11Buffer,
    sampler: ID3D11SamplerState,
}

impl FxRunner {
    pub fn new(gpu: &Gpu, width: u32, height: u32) -> Result<FxRunner> {
        let d = &gpu.device;
        let (mut vs, mut cbuf, mut sampler) = (None, None, None);
        unsafe {
            d.CreateVertexShader(&compile(VS, "vs", "vs_5_0")?, None, Some(&mut vs))?;
            d.CreateBuffer(
                &D3D11_BUFFER_DESC { ByteWidth: 96, Usage: D3D11_USAGE_DEFAULT, BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32, ..Default::default() },
                None,
                Some(&mut cbuf),
            )?;
            d.CreateSamplerState(
                &D3D11_SAMPLER_DESC {
                    Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
                    AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
                    AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
                    MaxLOD: f32::MAX,
                    ..Default::default()
                },
                Some(&mut sampler),
            )?;
        }
        Ok(FxRunner {
            width,
            height,
            targets: [RenderTarget::new(gpu, width, height)?, RenderTarget::new(gpu, width, height)?],
            next: 0,
            vs: vs.context("vs")?,
            cbuf: cbuf.context("cbuf")?,
            sampler: sampler.context("sampler")?,
        })
    }

    /// One pass of `program` over `input` (`second` = previous output for feedback, or the
    /// destination for transitions). The result stays valid until the pass after next.
    pub fn pass(&mut self, gpu: &Gpu, program: &FxProgram, input: &Texture, second: Option<&Texture>, p: &FxParams) -> Texture {
        let target = self.targets[self.next].rtv.clone();
        let out = self.targets[self.next].as_texture();
        self.next = 1 - self.next;
        self.run(gpu, program, input, second, p, &target, None);
        out
    }

    /// The same pass, drawn straight onto `rtv` with `state` (premultiplied output × `opacity`,
    /// see `evj_core::effect::build_shader`). Used for a layer's last pass.
    #[allow(clippy::too_many_arguments)]
    pub fn pass_blend(&mut self, gpu: &Gpu, program: &FxProgram, input: &Texture, second: Option<&Texture>, p: &FxParams, rtv: &ID3D11RenderTargetView, state: &ID3D11BlendState, opacity: f32) {
        self.run(gpu, program, input, second, p, rtv, Some((state, opacity)));
    }

    /// The pass drawn into another target (e.g. a layer tap), plain output.
    pub fn pass_to(&mut self, gpu: &Gpu, program: &FxProgram, input: &Texture, second: Option<&Texture>, p: &FxParams, rtv: &ID3D11RenderTargetView) {
        self.run(gpu, program, input, second, p, rtv, None);
    }

    #[allow(clippy::too_many_arguments)]
    fn run(&self, gpu: &Gpu, program: &FxProgram, input: &Texture, second: Option<&Texture>, p: &FxParams, target: &ID3D11RenderTargetView, blend: Option<(&ID3D11BlendState, f32)>) {
        let mut cb = [0f32; 24];
        cb[..4].copy_from_slice(&[p.time, p.beat, self.width as f32, self.height as f32]);
        cb[4] = p.progress;
        // evj_pad.x: > 0 = premultiplied output scaled by this opacity (fixed-function blend)
        cb[5] = blend.map_or(0.0, |(_, o)| o.max(1e-6));
        cb[8..].copy_from_slice(&p.values);
        let ctx = &gpu.ctx;
        unsafe {
            ctx.UpdateSubresource(&self.cbuf, 0, None, cb.as_ptr().cast(), 0, 0);
            ctx.OMSetRenderTargets(Some(&[Some(target.clone())]), None);
            ctx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: self.width as f32,
                Height: self.height as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            ctx.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx.IASetInputLayout(None);
            ctx.VSSetShader(&self.vs, None);
            ctx.PSSetShader(&program.ps, None);
            ctx.PSSetConstantBuffers(0, Some(&[Some(self.cbuf.clone())]));
            ctx.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            let second = second.map(|t| t.srv.clone()).or_else(|| Some(input.srv.clone()));
            ctx.PSSetShaderResources(0, Some(&[Some(input.srv.clone()), second]));
            if let Some((state, _)) = blend {
                ctx.OMSetBlendState(state, None, 0xffff_ffff);
            }
            ctx.Draw(3, 0);
            if blend.is_some() {
                ctx.OMSetBlendState(None, None, 0xffff_ffff);
            }
            ctx.PSSetShaderResources(0, Some(&[None, None]));
        }
    }

    /// Keeps `out` as next frame's `PREV` for a feedback effect.
    pub fn keep(&self, gpu: &Gpu, out: &Texture, history: &RenderTarget) {
        unsafe { gpu.ctx.CopyResource(&history.tex, &out.tex) };
    }

    pub fn readback(&self, gpu: &Gpu, tex: &Texture) -> Result<Vec<u8>> {
        let t = self.targets.iter().find(|t| t.tex == tex.tex).context("not one of this runner's targets")?;
        t.readback(gpu)
    }
}
