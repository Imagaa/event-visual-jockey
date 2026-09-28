use evj_core::effect::{EffectMeta, EffectRef, ParamValue, build_shader, parse_meta, resolve};
use evj_core::lfo::{Lfo, Wave};

const SRC: &str = r#"/*@evj
{ "name": "Twist", "params": [
    { "name": "amount", "min": 0, "max": 2, "default": 0.5 },
    { "name": "angle", "min": -180, "max": 180, "default": 0 }
] }
*/
float4 effect(float2 uv) { return SRC(uv) * amount; }
"#;

#[test]
fn parses_header() {
    let m = parse_meta(SRC).unwrap();
    assert_eq!(m.name, "Twist");
    assert_eq!(m.params.len(), 2);
    assert_eq!((m.params[1].min, m.params[1].max, m.params[1].default), (-180.0, 180.0, 0.0));
    assert!(!m.feedback);
}

#[test]
fn header_errors_are_clear() {
    assert!(parse_meta("float4 effect(float2 uv) { return 0; }").unwrap_err().to_string().contains("/*@evj"));
    let bad_name = SRC.replace("\"amount\"", "\"2amount\"");
    assert!(parse_meta(&bad_name).unwrap_err().to_string().contains("2amount"));
    let many: Vec<String> = (0..17).map(|i| format!(r#"{{"name":"p{i}","min":0,"max":1,"default":0}}"#)).collect();
    let too_many = format!("/*@evj {{\"name\":\"X\",\"params\":[{}]}} */", many.join(","));
    assert!(parse_meta(&too_many).is_err());
    assert!(parse_meta("/*@evj { not json } */").is_err());
}

#[test]
fn shader_source_maps_params_to_cbuffer() {
    let m = parse_meta(SRC).unwrap();
    let hlsl = build_shader(&m, SRC, "twist.hlsl");
    assert!(hlsl.contains("#define amount evj_p[0].x"));
    assert!(hlsl.contains("#define angle evj_p[0].y"));
    assert!(hlsl.contains("#line 1 \"twist.hlsl\""));
    assert!(hlsl.contains("evj_main"));
}

#[test]
fn resolve_defaults_overrides_and_lfo() {
    let m: EffectMeta = parse_meta(SRC).unwrap();
    let mut e = EffectRef::new("Twist");
    assert_eq!(&resolve(&m, &e, 0.0)[..2], &[0.5, 0.0]);
    e.params.push(ParamValue { name: "angle".into(), value: 999.0, lfo: None });
    assert_eq!(resolve(&m, &e, 0.0)[1], 180.0, "clamped to range");
    e.params[0] = ParamValue { name: "angle".into(), value: 0.0, lfo: Some(Lfo { wave: Wave::Saw, beats: 1.0, min: 0.0, max: 1.0 }) };
    assert!((resolve(&m, &e, 0.25)[1] - (-90.0)).abs() < 1e-3, "LFO spans the param range");
}

#[test]
fn swizzle_like_param_names_are_rejected() {
    for bad in ["x", "y", "xy", "rgba", "w"] {
        let src = SRC.replace("\"angle\"", &format!("\"{bad}\""));
        let err = parse_meta(&src).unwrap_err().to_string();
        assert!(err.contains(bad) && err.contains("swizzle"), "{bad}: {err}");
    }
    assert!(parse_meta(&SRC.replace("\"angle\"", "\"offset_x\"")).is_ok());
}

#[test]
fn transitions_mix_in_premultiplied_alpha() {
    let src = r#"/*@evj { "name": "X", "kind": "transition" } */ float4 effect(float2 uv) { return lerp(SRC(uv), DST(uv), PROGRESS); }"#;
    let hlsl = build_shader(&parse_meta(src).unwrap(), src, "x.hlsl");
    assert!(hlsl.contains("c.rgb * c.a"), "inputs premultiplied");
    assert!(hlsl.contains("r.rgb / r.a"), "output back to straight alpha");
    let fx = build_shader(&parse_meta(SRC).unwrap(), SRC, "t.hlsl");
    assert!(!fx.contains("r.rgb / r.a"), "effects stay straight");
}
