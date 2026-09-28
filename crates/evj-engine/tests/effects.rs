use evj_core::effect::{EffectRef, resolve};
use evj_engine::fx::Library;
use evj_render::{DeviceKind, FxParams, FxRunner, Gpu, Texture};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM;

const W: u32 = 32;
const H: u32 = 16;

/// Gradient + noise so geometric effects visibly move things.
fn pattern(gpu: &Gpu) -> (Texture, Vec<u8>) {
    let mut px = Vec::new();
    for y in 0..H {
        for x in 0..W {
            let n = ((x * 7919 + y * 104729) % 97) as u8;
            px.extend_from_slice(&[(x * 8) as u8, (y * 16) as u8, n.wrapping_mul(2), 255]);
        }
    }
    let t = Texture::new_color(gpu, DXGI_FORMAT_R8G8B8A8_UNORM, W, H).unwrap();
    t.upload(&gpu.ctx, &px, W * 4);
    (t, px)
}

fn run(name: &str) -> (Vec<u8>, Vec<u8>) {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let lib = Library::new(&gpu, vec![]);
    let e = lib.find(name).unwrap_or_else(|| panic!("{name} missing"));
    let program = e.program.as_ref().unwrap_or_else(|| panic!("{name}: {:?}", e.error));
    let (src, px) = pattern(&gpu);
    let mut fx = FxRunner::new(&gpu, W, H).unwrap();
    let params = FxParams { time: 0.25, beat: 0.25, progress: 0.0, values: resolve(&e.meta, &EffectRef::new(name), 0.25) };
    let out = fx.pass(&gpu, program, &src, None, &params);
    (fx.readback(&gpu, &out).unwrap(), px)
}

fn max_diff(a: &[u8], b: &[u8]) -> i32 {
    a.iter().zip(b).map(|(x, y)| (*x as i32 - *y as i32).abs()).max().unwrap_or(0)
}

#[test]
fn every_builtin_compiles() {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let lib = Library::new(&gpu, vec![]);
    assert!(lib.entries.len() >= 16, "{}", lib.entries.len());
    for e in &lib.entries {
        assert!(e.program.is_some(), "{}: {:?}", e.meta.name, e.error);
    }
}

#[test]
fn defaults_are_identity() {
    for name in [
        "Transform", "Brightness/Contrast", "Hue Shift", "Colorize", "Blur", "RGB Shift", "Pixelate", "Posterize", "Edge", "Strobe", "Wave",
        "Feedback", "Vignette",
    ] {
        let (out, input) = run(name);
        assert!(max_diff(&out, &input) <= 3, "{name} changed the image at default settings (max diff {})", max_diff(&out, &input));
    }
}

#[test]
fn some_effects_change_the_image_by_default() {
    for name in ["Invert", "Kaleidoscope", "Mirror"] {
        let (out, input) = run(name);
        assert!(max_diff(&out, &input) > 20, "{name} did nothing");
    }
}

#[test]
fn custom_folder_hot_reload_and_errors() {
    let dir = std::env::temp_dir().join(format!("evj-fx-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let good = r#"/*@evj { "name": "My Red", "params": [] } */
float4 effect(float2 uv) { return float4(1, 0, 0, 1); }"#;
    std::fs::write(dir.join("red.hlsl"), good).unwrap();
    std::fs::write(dir.join("broken.hlsl"), "/*@evj { \"name\": \"Broken\" } */ float4 effect(float2 uv) { return oops; }").unwrap();

    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let mut lib = Library::new(&gpu, vec![dir.clone()]);
    assert!(lib.find("My Red").and_then(|e| e.program.as_ref()).is_some());
    let broken = lib.find("Broken").expect("broken effect listed with its error");
    assert!(broken.program.is_none() && broken.error.as_deref().unwrap_or("").contains("oops"), "{:?}", broken.error);

    let v = lib.version;
    std::thread::sleep(std::time::Duration::from_millis(1100)); // mtime resolution
    std::fs::write(dir.join("red.hlsl"), good.replace("My Red", "My Green")).unwrap();
    lib.rescan(&gpu);
    assert!(lib.version > v);
    assert!(lib.find("My Green").is_some() && lib.find("My Red").is_none());

    std::fs::remove_file(dir.join("broken.hlsl")).unwrap();
    lib.rescan(&gpu);
    assert!(lib.find("Broken").is_none());
}

fn solid(gpu: &Gpu, rgba: [u8; 4]) -> Texture {
    let t = Texture::new_color(gpu, DXGI_FORMAT_R8G8B8A8_UNORM, W, H).unwrap();
    t.upload(&gpu.ctx, &rgba.repeat((W * H) as usize), W * 4);
    t
}

#[test]
fn transitions_start_at_source_and_end_at_destination() {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let lib = Library::new(&gpu, vec![]);
    let names: Vec<String> = lib.transitions().map(|e| e.meta.name.clone()).collect();
    assert!(names.len() >= 12, "{names:?}");
    let (from, _) = pattern(&gpu);
    let from_px = {
        let mut fx = FxRunner::new(&gpu, W, H).unwrap();
        let copy = lib.find("Invert").unwrap();
        // two inversions = the pattern itself, read back through the runner
        let p = FxParams { time: 0.0, beat: 0.0, progress: 0.0, values: resolve(&copy.meta, &EffectRef::new("Invert"), 0.0) };
        let a = fx.pass(&gpu, copy.program.as_ref().unwrap(), &from, None, &p);
        let b = fx.pass(&gpu, copy.program.as_ref().unwrap(), &a, None, &p);
        fx.readback(&gpu, &b).unwrap()
    };
    let to = solid(&gpu, [20, 200, 90, 255]);
    let to_px = [20u8, 200, 90, 255].repeat((W * H) as usize);
    for name in names {
        let e = lib.find_transition(&name).unwrap();
        let program = e.program.as_ref().unwrap_or_else(|| panic!("{name}: {:?}", e.error));
        let mut fx = FxRunner::new(&gpu, W, H).unwrap();
        for (progress, want) in [(0.0f32, &from_px), (1.0, &to_px)] {
            let p = FxParams { time: 0.0, beat: 0.0, progress, values: resolve(&e.meta, &EffectRef::new(&name), 0.0) };
            let out = fx.pass(&gpu, program, &from, Some(&to), &p);
            let px = fx.readback(&gpu, &out).unwrap();
            assert!(max_diff(&px, want) <= 3, "{name} at progress {progress}: max diff {}", max_diff(&px, want));
        }
    }
}

#[test]
fn effect_and_transition_names_do_not_collide() {
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let lib = Library::new(&gpu, vec![]);
    assert_eq!(lib.find("Pixelate").unwrap().meta.kind, evj_core::effect::Kind::Effect);
    assert_eq!(lib.find_transition("Pixelate").unwrap().meta.kind, evj_core::effect::Kind::Transition);
}

#[test]
fn custom_transition_is_imported_from_the_folder() {
    let dir = std::env::temp_dir().join(format!("evj-tr-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("iris.hlsl"),
        r#"/*@evj { "name": "Iris", "kind": "transition", "params": [] } */
float4 effect(float2 uv) { return length(uv - 0.5) < PROGRESS * 0.75 ? DST(uv) : SRC(uv); }"#,
    )
    .unwrap();
    let gpu = Gpu::new(DeviceKind::Warp).unwrap();
    let lib = Library::new(&gpu, vec![dir]);
    let iris = lib.find_transition("Iris").expect("custom transition listed");
    assert!(iris.program.is_some(), "{:?}", iris.error);
    assert!(lib.find("Iris").is_none(), "not offered as an effect");
}
