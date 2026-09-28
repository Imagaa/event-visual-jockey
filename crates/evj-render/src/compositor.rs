//! Layer compositing: each layer is drawn into a transparent layer target (fit rect), then blended
//! onto the composition with one of the blend modes (ping-pong between two targets).
use crate::{Blitter, Gpu, RenderTarget, Shade, Texture, compile};
use anyhow::{Context, Result};
use windows::Win32::Graphics::Direct3D::D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D11::*;

const HLSL: &str = r#"
cbuffer P : register(b0) { uint mode; float opacity; float2 pad; };
Texture2D base : register(t0);
Texture2D layer : register(t1);
float4 vs(uint id : SV_VertexID) : SV_Position {
    float2 t = float2((id << 1) & 2, id & 2);
    return float4(t * float2(2, -2) + float2(-1, 1), 0, 1);
}
float3 blendf(float3 b, float3 s) {
    switch (mode) {
        case 1: return saturate(b + s);
        case 2: return 1 - (1 - b) * (1 - s);
        case 3: return b * s;
        case 4: return lerp(2 * b * s, 1 - 2 * (1 - b) * (1 - s), step(0.5, b));
        case 5: return abs(b - s);
        case 6: return max(b, s);
        case 7: return min(b, s);
        case 8: return saturate(b - s);
        default: return s; // 0 alpha, 9 luma key
    }
}
float4 ps_blend(float4 pos : SV_Position) : SV_Target {
    int3 p = int3(pos.xy, 0);
    float4 b = base.Load(p);
    float4 s = layer.Load(p);
    float a = s.a * opacity;
    if (mode == 9) a *= dot(s.rgb, float3(0.2126, 0.7152, 0.0722));
    return float4(lerp(b.rgb, blendf(b.rgb, s.rgb), a), 1);
}
"#;

/// Where a `sw`×`sh` source lands in a `dw`×`dh` target: 0 fit (letterbox), 1 fill (crop), 2 stretch.
pub fn fit_rect(sw: u32, sh: u32, dw: u32, dh: u32, mode: u32) -> [f32; 4] {
    let (sw, sh, dw, dh) = (sw.max(1) as f32, sh.max(1) as f32, dw as f32, dh as f32);
    if mode == 2 {
        return [0.0, 0.0, dw, dh];
    }
    let (kx, ky) = (dw / sw, dh / sh);
    let k = if mode == 1 { kx.max(ky) } else { kx.min(ky) };
    let (w, h) = (sw * k, sh * k);
    [(dw - w) / 2.0, (dh - h) / 2.0, w, h]
}

pub struct Compositor {
    pub width: u32,
    pub height: u32,
    rts: [RenderTarget; 2],
    layer_rt: RenderTarget,
    cur: usize,
    blitter: Blitter,
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    cbuf: ID3D11Buffer,
    /// Alpha, Screen, Multiply as fixed-function blends of premultiplied colour P = s·a:
    /// Alpha P + b(1-a) · Screen P(1-b) + b · Multiply P·b + b(1-a). Composition alpha stays 1.
    fixed: [ID3D11BlendState; 3],
}

fn blend_state(gpu: &Gpu, src: D3D11_BLEND, dst: D3D11_BLEND) -> Result<ID3D11BlendState> {
    let mut desc = D3D11_BLEND_DESC::default();
    desc.RenderTarget[0] = D3D11_RENDER_TARGET_BLEND_DESC {
        BlendEnable: true.into(),
        SrcBlend: src,
        DestBlend: dst,
        BlendOp: D3D11_BLEND_OP_ADD,
        SrcBlendAlpha: D3D11_BLEND_ZERO,
        DestBlendAlpha: D3D11_BLEND_ONE,
        BlendOpAlpha: D3D11_BLEND_OP_ADD,
        RenderTargetWriteMask: D3D11_COLOR_WRITE_ENABLE_ALL.0 as u8,
    };
    let mut s = None;
    unsafe { gpu.device.CreateBlendState(&desc, Some(&mut s))? };
    s.context("blend state")
}

impl Compositor {
    pub fn new(gpu: &Gpu, width: u32, height: u32) -> Result<Compositor> {
        let d = &gpu.device;
        let (mut vs, mut ps, mut cbuf) = (None, None, None);
        unsafe {
            d.CreateVertexShader(&compile(HLSL, "vs", "vs_5_0")?, None, Some(&mut vs))?;
            d.CreatePixelShader(&compile(HLSL, "ps_blend", "ps_5_0")?, None, Some(&mut ps))?;
            d.CreateBuffer(
                &D3D11_BUFFER_DESC { ByteWidth: 16, Usage: D3D11_USAGE_DEFAULT, BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32, ..Default::default() },
                None,
                Some(&mut cbuf),
            )?;
        }
        Ok(Compositor {
            width,
            height,
            rts: [RenderTarget::new(gpu, width, height)?, RenderTarget::new(gpu, width, height)?],
            layer_rt: RenderTarget::new(gpu, width, height)?,
            cur: 0,
            blitter: Blitter::new(gpu)?,
            vs: vs.context("vs")?,
            ps: ps.context("ps")?,
            cbuf: cbuf.context("cbuf")?,
            fixed: [
                blend_state(gpu, D3D11_BLEND_ONE, D3D11_BLEND_INV_SRC_ALPHA)?,
                blend_state(gpu, D3D11_BLEND_INV_DEST_COLOR, D3D11_BLEND_ONE)?,
                blend_state(gpu, D3D11_BLEND_DEST_COLOR, D3D11_BLEND_INV_SRC_ALPHA)?,
            ],
        })
    }

    /// Starts a frame: composition = opaque black.
    pub fn begin(&mut self, ctx: &ID3D11DeviceContext) {
        unsafe { ctx.ClearRenderTargetView(&self.rts[self.cur].rtv, &[0.0, 0.0, 0.0, 1.0]) };
    }

    /// Draws `src` into `rect` (see [`fit_rect`]) and blends it over the composition.
    pub fn layer(&mut self, ctx: &ID3D11DeviceContext, src: &Texture, shade: Shade, rect: [f32; 4], blend: u32, opacity: f32) {
        let t = self.prepare(ctx, src, shade, rect);
        self.blend(ctx, &t, blend, opacity);
    }

    /// Draws `src` into `rect` on a transparent composition-sized layer (for effects before blending).
    pub fn prepare(&mut self, ctx: &ID3D11DeviceContext, src: &Texture, shade: Shade, rect: [f32; 4]) -> Texture {
        unsafe { ctx.ClearRenderTargetView(&self.layer_rt.rtv, &[0.0, 0.0, 0.0, 0.0]) };
        self.blitter.draw(ctx, src, shade, &self.layer_rt.rtv, rect);
        self.layer_rt.as_texture()
    }

    /// Blends a composition-sized layer image over the composition.
    pub fn blend(&mut self, ctx: &ID3D11DeviceContext, layer: &Texture, blend: u32, opacity: f32) {
        let next = 1 - self.cur;
        let params: [u32; 4] = [blend, opacity.to_bits(), 0, 0];
        unsafe {
            ctx.UpdateSubresource(&self.cbuf, 0, None, params.as_ptr().cast(), 0, 0);
            ctx.OMSetRenderTargets(Some(&[Some(self.rts[next].rtv.clone())]), None);
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
            ctx.PSSetShader(&self.ps, None);
            ctx.PSSetConstantBuffers(0, Some(&[Some(self.cbuf.clone())]));
            ctx.PSSetShaderResources(0, Some(&[Some(self.rts[self.cur].srv.clone()), Some(layer.srv.clone())]));
            ctx.Draw(3, 0);
            ctx.PSSetShaderResources(0, Some(&[None, None]));
        }
        self.cur = next;
    }

    pub fn output(&self) -> &RenderTarget {
        &self.rts[self.cur]
    }

    /// For modes a fixed-function blend reproduces (Alpha, Screen, Multiply): the composition
    /// target and blend state, so a layer's last pass can draw premultiplied colour straight
    /// onto the composition — no layer image, no separate blend pass.
    pub fn fixed(&self, mode: u32) -> Option<(ID3D11RenderTargetView, ID3D11BlendState)> {
        let state = match mode {
            0 => &self.fixed[0],
            2 => &self.fixed[1],
            3 => &self.fixed[2],
            _ => return None,
        };
        Some((self.rts[self.cur].rtv.clone(), state.clone()))
    }

    /// Draws `src` into `rect` blended straight onto the composition; false for modes that need
    /// the shader blend (see [`Compositor::fixed`]).
    pub fn blit_fixed(&self, ctx: &ID3D11DeviceContext, src: &Texture, shade: Shade, rect: [f32; 4], mode: u32, opacity: f32) -> bool {
        let Some((rtv, state)) = self.fixed(mode) else { return false };
        self.blitter.draw_blend(ctx, src, shade, &rtv, rect, &state, opacity);
        true
    }
}
