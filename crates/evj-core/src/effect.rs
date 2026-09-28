//! Effect (and transition) shaders: an HLSL file with a JSON header, e.g.
//!
//! ```text
//! /*@evj { "name": "Invert", "params": [ { "name": "amount", "min": 0, "max": 1, "default": 1 } ] } */
//! float4 effect(float2 uv) { float4 c = SRC(uv); return float4(lerp(c.rgb, 1 - c.rgb, amount), c.a); }
//! ```
//!
//! Parameters are usable by name; `SRC(uv)`, `PREV(uv)` (feedback), `TIME`, `BEAT`, `RESOLUTION` come from the prelude.
use crate::lfo::Lfo;
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};

pub const MAX_PARAMS: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamDef {
    pub name: String,
    pub min: f32,
    pub max: f32,
    pub default: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[default]
    Effect,
    /// Two inputs (`SRC` = from, `DST(uv)` = to) and `PROGRESS` 0..1.
    Transition,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EffectMeta {
    pub name: String,
    #[serde(default)]
    pub params: Vec<ParamDef>,
    /// Needs its own previous output (`PREV(uv)`).
    #[serde(default)]
    pub feedback: bool,
    #[serde(default)]
    pub kind: Kind,
    /// Used internally (e.g. the presenter pointer): not offered in the effect lists.
    #[serde(default)]
    pub hidden: bool,
}

/// A parameter value stored in the project (by name, so effects can gain parameters later).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamValue {
    pub name: String,
    pub value: f64,
    #[serde(default)]
    pub lfo: Option<Lfo>,
}

/// An effect in a clip / layer / composition chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct EffectRef {
    pub name: String,
    pub bypass: bool,
    pub params: Vec<ParamValue>,
}

impl EffectRef {
    pub fn new(name: &str) -> EffectRef {
        EffectRef { name: name.to_string(), bypass: false, params: Vec::new() }
    }

    pub fn param_mut(&mut self, name: &str, default: f64) -> &mut ParamValue {
        if let Some(i) = self.params.iter().position(|p| p.name == name) {
            return &mut self.params[i];
        }
        self.params.push(ParamValue { name: name.to_string(), value: default, lfo: None });
        self.params.last_mut().expect("just pushed")
    }
}

/// `x`, `xy`, `rgba`... would be rewritten inside every swizzle by the parameter `#define`.
fn is_swizzle(s: &str) -> bool {
    s.len() <= 4 && (s.chars().all(|c| "xyzw".contains(c)) || s.chars().all(|c| "rgba".contains(c)))
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_ascii_alphabetic() || f == '_') && c.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub fn parse_meta(src: &str) -> Result<EffectMeta> {
    let body = src.trim_start().strip_prefix("/*@evj").context("effect file must start with a /*@evj { ... } */ header")?;
    let end = body.find("*/").context("unterminated /*@evj header")?;
    let meta: EffectMeta = serde_json::from_str(&body[..end]).context("invalid JSON in /*@evj header")?;
    ensure!(!meta.name.trim().is_empty(), "effect name is empty");
    ensure!(meta.params.len() <= MAX_PARAMS, "at most {MAX_PARAMS} parameters");
    for p in &meta.params {
        if !is_ident(&p.name) {
            bail!("parameter name '{}' is not a valid identifier", p.name);
        }
        if is_swizzle(&p.name) {
            bail!("parameter name '{}' clashes with HLSL swizzles (.x/.rgba) — use e.g. '{}_pos'", p.name, p.name);
        }
        ensure!(p.min <= p.max, "parameter '{}': min > max", p.name);
    }
    Ok(meta)
}

const PRELUDE: &str = r#"
cbuffer EvjFx : register(b0) { float evj_time; float evj_beat; float2 evj_resolution; float evj_progress; float3 evj_pad; float4 evj_p[4]; };
Texture2D evj_src : register(t0);
Texture2D evj_prev : register(t1);
SamplerState evj_samp : register(s0);
#define TIME evj_time
#define BEAT evj_beat
#define RESOLUTION evj_resolution
#define PROGRESS evj_progress
static const float PI = 3.14159265359;
"#;

const EFFECT_IO: &str = r#"
float4 SRC(float2 uv) { return evj_src.Sample(evj_samp, uv); }
float4 PREV(float2 uv) { return evj_prev.Sample(evj_samp, uv); }
"#;

// Transitions mix premultiplied colours (so fading to transparent does not darken) and
// hand straight alpha back to the compositor.
const TRANSITION_IO: &str = r#"
float4 evj_pm(float4 c) { return float4(c.rgb * c.a, c.a); }
float4 SRC(float2 uv) { float4 c = evj_src.Sample(evj_samp, uv); return evj_pm(c); }
float4 DST(float2 uv) { float4 c = evj_prev.Sample(evj_samp, uv); return evj_pm(c); }
"#;

/// Full HLSL: prelude + parameter defines + the file (errors keep the file's own line numbers).
pub fn build_shader(meta: &EffectMeta, src: &str, file_name: &str) -> String {
    let mut s = String::from(PRELUDE);
    s += if meta.kind == Kind::Transition { TRANSITION_IO } else { EFFECT_IO };
    for (i, p) in meta.params.iter().enumerate() {
        s += &format!("#define {} evj_p[{}].{}\n", p.name, i / 4, ["x", "y", "z", "w"][i % 4]);
    }
    s += &format!("#line 1 \"{}\"\n", file_name.replace('"', ""));
    s += src;
    s += match meta.kind {
        // evj_pad.x > 0: the pass is drawn straight onto the composition with a fixed-function
        // blend — hand back premultiplied colour scaled by that opacity.
        Kind::Effect => "\nfloat4 evj_main(float4 pos : SV_Position, float2 uv : TEXCOORD0) : SV_Target { float4 c = effect(uv); if (evj_pad.x > 0) { float a = saturate(c.a) * evj_pad.x; return float4(saturate(c.rgb) * a, a); } return c; }\n",
        Kind::Transition => "\nfloat4 evj_main(float4 pos : SV_Position, float2 uv : TEXCOORD0) : SV_Target { float4 r = effect(uv); if (evj_pad.x > 0) return saturate(r) * evj_pad.x; return r.a > 1e-5 ? float4(saturate(r.rgb / r.a), r.a) : 0; }\n",
    };
    s
}

/// Parameter values for the shader at `beat`: defaults, overridden by stored values or LFOs, clamped.
pub fn resolve(meta: &EffectMeta, e: &EffectRef, beat: f64) -> [f32; MAX_PARAMS] {
    let mut out = [0.0; MAX_PARAMS];
    for (i, d) in meta.params.iter().enumerate() {
        let v = match e.params.iter().find(|p| p.name == d.name) {
            Some(ParamValue { lfo: Some(l), .. }) => d.min as f64 + (d.max - d.min) as f64 * l.value(beat),
            Some(p) => p.value,
            None => d.default as f64,
        };
        out[i] = (v as f32).clamp(d.min, d.max);
    }
    out
}
