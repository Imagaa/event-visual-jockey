use crate::{Gpu, Texture, compile};
use anyhow::{Context, Result};
use windows::Win32::Graphics::Direct3D::D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST;
use windows::Win32::Graphics::Direct3D11::*;

const HLSL: &str = r#"
// `blend` > 0: premultiplied output scaled by that opacity (drawn with a fixed-function blend state).
cbuffer Params : register(b0) { float2 uv_scale; float bt709; float blend; float4 crop; };
Texture2D src : register(t0);
Texture2D src_uv : register(t1);
SamplerState samp : register(s0);
struct V { float4 pos : SV_Position; float2 uv : TEXCOORD0; };
V vs(uint id : SV_VertexID) {
    V o; float2 t = float2((id << 1) & 2, id & 2);
    o.pos = float4(t * float2(2, -2) + float2(-1, 1), 0, 1);
    o.uv = (crop.xy + t * crop.zw) * uv_scale; return o;
}
float4 fin(float4 c) {
    if (blend > 0) { float a = saturate(c.a) * blend; return float4(saturate(c.rgb) * a, a); }
    return c;
}
// Samples stay half a texel inside the crop, so slices never bleed neighbouring pixels.
float4 ps_rgba(V i) : SV_Target {
    float w, h; src.GetDimensions(w, h);
    float2 hp = 0.5 / float2(w, h);
    float2 lo = crop.xy * uv_scale + hp, hi = (crop.xy + crop.zw) * uv_scale - hp;
    return fin(src.Sample(samp, clamp(i.uv, lo, max(lo, hi))));
}
// HAP Q: scaled YCoCg in BC3 (reference: Vidvox ScaledCoCgYToRGBA.frag)
float4 ps_ycocg(V i) : SV_Target {
    float4 c = src.Sample(samp, i.uv) - float4(0.50196078431373, 0.50196078431373, 0, 0);
    float scale = c.z * (255.0 / 8.0) + 1.0;
    float co = c.x / scale, cg = c.y / scale, y = c.w;
    return fin(float4(y + co - cg, y + cg, y - co - cg, 1));
}
// NV12, limited range, BT.709 or BT.601
float4 ps_nv12(V i) : SV_Target {
    float y = (src.Sample(samp, i.uv).r - 16.0 / 255.0) * (255.0 / 219.0);
    float2 c = (src_uv.Sample(samp, i.uv).rg - 128.0 / 255.0) * (255.0 / 224.0);
    float3 rgb = bt709 > 0.5
        ? float3(y + 1.5748 * c.y, y - 0.1873 * c.x - 0.4681 * c.y, y + 1.8556 * c.x)
        : float3(y + 1.402 * c.y, y - 0.344136 * c.x - 0.714136 * c.y, y + 1.772 * c.x);
    return fin(float4(saturate(rgb), 1));
}
"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shade {
    Rgba,
    YCoCg,
    Nv12 { bt709: bool },
}

pub struct Blitter {
    vs: ID3D11VertexShader,
    ps_rgba: ID3D11PixelShader,
    ps_ycocg: ID3D11PixelShader,
    ps_nv12: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    cbuf: ID3D11Buffer,
}

impl Blitter {
    pub fn new(gpu: &Gpu) -> Result<Blitter> {
        let d = &gpu.device;
        let (mut vs, mut ps_rgba, mut ps_ycocg, mut ps_nv12, mut sampler, mut cbuf) = (None, None, None, None, None, None);
        unsafe {
            d.CreateVertexShader(&compile(HLSL, "vs", "vs_5_0")?, None, Some(&mut vs))?;
            d.CreatePixelShader(&compile(HLSL, "ps_rgba", "ps_5_0")?, None, Some(&mut ps_rgba))?;
            d.CreatePixelShader(&compile(HLSL, "ps_ycocg", "ps_5_0")?, None, Some(&mut ps_ycocg))?;
            d.CreatePixelShader(&compile(HLSL, "ps_nv12", "ps_5_0")?, None, Some(&mut ps_nv12))?;
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
            d.CreateBuffer(
                &D3D11_BUFFER_DESC {
                    ByteWidth: 32,
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                    ..Default::default()
                },
                None,
                Some(&mut cbuf),
            )?;
        }
        Ok(Blitter {
            vs: vs.context("vs")?,
            ps_rgba: ps_rgba.context("ps_rgba")?,
            ps_ycocg: ps_ycocg.context("ps_ycocg")?,
            ps_nv12: ps_nv12.context("ps_nv12")?,
            sampler: sampler.context("sampler")?,
            cbuf: cbuf.context("cbuf")?,
        })
    }

    pub fn draw(&self, ctx: &ID3D11DeviceContext, src: &Texture, shade: Shade, rtv: &ID3D11RenderTargetView, viewport: [f32; 4]) {
        self.draw_region(ctx, src, shade, rtv, viewport, [0.0, 0.0, 1.0, 1.0]);
    }

    /// Draws the `crop` part of `src` (x, y, w, h in 0..1) into `viewport` (pixels).
    pub fn draw_region(&self, ctx: &ID3D11DeviceContext, src: &Texture, shade: Shade, rtv: &ID3D11RenderTargetView, viewport: [f32; 4], crop: [f32; 4]) {
        self.draw_with(ctx, src, shade, rtv, viewport, crop, None);
    }

    /// Draws `src` into `viewport` with a fixed-function blend state; the shader outputs
    /// premultiplied colour scaled by `opacity`.
    pub fn draw_blend(&self, ctx: &ID3D11DeviceContext, src: &Texture, shade: Shade, rtv: &ID3D11RenderTargetView, viewport: [f32; 4], state: &ID3D11BlendState, opacity: f32) {
        self.draw_with(ctx, src, shade, rtv, viewport, [0.0, 0.0, 1.0, 1.0], Some((state, opacity)));
    }

    #[allow(clippy::too_many_arguments)]
    fn draw_with(&self, ctx: &ID3D11DeviceContext, src: &Texture, shade: Shade, rtv: &ID3D11RenderTargetView, viewport: [f32; 4], crop: [f32; 4], blend: Option<(&ID3D11BlendState, f32)>) {
        let (ps, bt709) = match shade {
            Shade::Rgba => (&self.ps_rgba, false),
            Shade::YCoCg => (&self.ps_ycocg, false),
            Shade::Nv12 { bt709 } => (&self.ps_nv12, bt709),
        };
        // opacity 0 would read as "no blend": keep it just above
        let blend_value = blend.map_or(0.0, |(_, o)| o.max(1e-6));
        let params = [src.uv_scale[0], src.uv_scale[1], if bt709 { 1.0 } else { 0.0 }, blend_value, crop[0], crop[1], crop[2], crop[3]];
        unsafe {
            ctx.UpdateSubresource(&self.cbuf, 0, None, params.as_ptr().cast(), 0, 0);
            ctx.OMSetRenderTargets(Some(&[Some(rtv.clone())]), None);
            ctx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: viewport[0],
                TopLeftY: viewport[1],
                Width: viewport[2],
                Height: viewport[3],
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            ctx.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            ctx.IASetInputLayout(None);
            ctx.VSSetShader(&self.vs, None);
            ctx.VSSetConstantBuffers(0, Some(&[Some(self.cbuf.clone())]));
            ctx.PSSetShader(ps, None);
            ctx.PSSetConstantBuffers(0, Some(&[Some(self.cbuf.clone())]));
            ctx.PSSetShaderResources(0, Some(&[Some(src.srv.clone()), src.srv_uv.clone()]));
            ctx.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
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
}
