use evj_core::model::{BlendMode, Clip, Layer, PlayMode};
use evj_engine::{Command, Engine, EngineConfig, Snapshot};
use evj_render::DeviceKind;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../evj-media/tests/fixtures").join(name)
}

fn engine() -> Engine {
    Engine::start(EngineConfig { device: DeviceKind::Warp, manual_clock: true, width: 8, height: 4, layers: 4, effect_folders: vec![], audio: false }).unwrap()
}

/// Steps the manual clock until `ok` holds (clips open asynchronously).
fn step_until(e: &Engine, dt: f64, ok: impl Fn(&Snapshot) -> bool) -> Snapshot {
    for _ in 0..500 {
        e.step(dt);
        let s = e.snapshot();
        if ok(&s) {
            return s;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("condition not reached: {:?}", e.snapshot());
}

fn pixels(e: &Engine) -> Vec<u8> {
    let (tx, rx) = channel();
    e.send(Command::Readback(tx));
    rx.recv().unwrap()
}

fn centre(px: &[u8]) -> [u8; 3] {
    let i = (1 * 8 + 4) * 4;
    [px[i], px[i + 1], px[i + 2]]
}

fn stretch(name: &str) -> Clip {
    Clip { fit: evj_core::model::FitMode::Stretch, ..Clip::new(fixture(name)) }
}

fn showing(layer: usize) -> impl Fn(&Snapshot) -> bool {
    move |s| s.layers[layer].has_frame
}

#[test]
fn image_fills_composition() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
    step_until(&e, 0.016, showing(0));
    e.step(0.016);
    let c = centre(&pixels(&e));
    assert!(c[0] > 250 && c[1] < 5 && c[2] < 5, "{c:?}");
    e.shutdown().unwrap();
}

#[test]
fn opacity_mixes_layers_and_clear_removes() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: stretch("blue_36x64.jpg"), transition: None });
    e.send(Command::Trigger { layer: 1, clip: stretch("red_64x36.png"), transition: None });
    e.send(Command::SetLayer { layer: 1, props: Layer { opacity: 0.5, ..Layer::default() } });
    step_until(&e, 0.016, |s| s.layers[0].has_frame && s.layers[1].has_frame);
    e.step(0.016);
    let c = centre(&pixels(&e));
    assert!((c[0] as i32 - 127).abs() < 8 && (c[2] as i32 - 127).abs() < 12, "{c:?}");

    e.send(Command::Clear { layer: 1, transition: None });
    e.step(0.016);
    let c = centre(&pixels(&e));
    assert!(c[0] < 10 && c[2] > 240, "{c:?}");
    e.shutdown().unwrap();
}

#[test]
fn bypass_and_solo() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: stretch("blue_36x64.jpg"), transition: None });
    e.send(Command::Trigger { layer: 1, clip: stretch("red_64x36.png"), transition: None });
    step_until(&e, 0.016, |s| s.layers[0].has_frame && s.layers[1].has_frame);

    e.send(Command::SetLayer { layer: 1, props: Layer { bypass: true, ..Layer::default() } });
    e.step(0.016);
    assert!(centre(&pixels(&e))[2] > 240, "bypassed red layer must not show");

    e.send(Command::SetLayer { layer: 1, props: Layer::default() });
    e.send(Command::SetLayer { layer: 0, props: Layer { solo: true, blend: BlendMode::Alpha, ..Layer::default() } });
    e.step(0.016);
    assert!(centre(&pixels(&e))[2] > 240, "only the solo (blue) layer shows");
    e.shutdown().unwrap();
}

#[test]
fn missing_file_reports_error_and_engine_keeps_working() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: Clip::new(PathBuf::from("C:/nope/missing.mp4")), transition: None });
    let s = step_until(&e, 0.016, |s| s.layers[0].error.is_some());
    assert!(s.layers[0].error.as_deref().unwrap_or("").contains("missing.mp4"));
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
    let s = step_until(&e, 0.016, showing(0));
    assert!(s.layers[0].error.is_none());
    e.shutdown().unwrap();
}

#[test]
fn hap_clip_position_follows_clock() {
    let e = engine();
    // 3 frames @30 fps = 0.1 s clip; ping-pong so position stays readable
    e.send(Command::Trigger { layer: 2, clip: Clip { mode: PlayMode::PingPong, ..Clip::new(fixture("hap1_64x48.mov")) }, transition: None });
    let s = step_until(&e, 0.0, showing(2));
    assert!((s.layers[2].duration - 0.1).abs() < 1e-6);
    let p0 = s.layers[2].pos;
    e.step(0.05);
    let p1 = e.snapshot().layers[2].pos;
    assert!((p1 - p0 - 0.05).abs() < 1e-6, "{p0} → {p1}");
    e.shutdown().unwrap();
}

#[test]
fn layer_count_can_change() {
    let e = engine();
    e.send(Command::SetLayerCount(6));
    e.step(0.016);
    assert_eq!(e.snapshot().layers.len(), 6);
    e.send(Command::SetLayerCount(2));
    e.step(0.016);
    assert_eq!(e.snapshot().layers.len(), 2);
    e.shutdown().unwrap();
}

#[test]
fn resolution_can_change_at_runtime() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
    step_until(&e, 0.016, showing(0));
    e.send(Command::SetResolution { width: 12, height: 3 });
    e.step(0.016);
    let px = pixels(&e);
    assert_eq!(px.len(), 12 * 3 * 4);
    assert!(px.chunks(4).all(|p| p[0] > 250), "clip re-rendered at the new size");
    assert_eq!(e.snapshot().resolution, (12, 3));
    e.shutdown().unwrap();
}

#[test]
fn same_resolution_keeps_the_preview() {
    let e = engine();
    let (tx, rx) = channel();
    e.send(Command::SharePreview(tx));
    rx.recv().unwrap().unwrap();
    e.step(0.016);
    assert!(e.snapshot().preview);
    e.send(Command::SetResolution { width: 8, height: 4 });
    e.step(0.016);
    assert!(e.snapshot().preview, "unchanged size must not drop the shared preview");
    e.send(Command::SetResolution { width: 16, height: 4 });
    e.step(0.016);
    assert!(!e.snapshot().preview, "new size: UI must ask for a new preview");
    e.shutdown().unwrap();
}

fn invert() -> evj_core::effect::EffectRef {
    evj_core::effect::EffectRef::new("Invert")
}

#[test]
fn tap_tempo_through_commands() {
    let e = engine();
    for _ in 0..4 {
        e.send(Command::Tap);
        e.step(0.5);
    }
    let s = e.snapshot();
    assert!((s.bpm - 120.0).abs() < 0.01, "{}", s.bpm);
    e.send(Command::SetBpm(90.0));
    e.send(Command::Resync);
    e.step(0.0);
    let s = e.snapshot();
    assert_eq!(s.bpm, 90.0);
    assert!(s.beat.abs() < 1e-9);
    e.shutdown().unwrap();
}

#[test]
fn layer_clip_and_composition_effects() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
    step_until(&e, 0.016, showing(0));

    // Layer effect: red → cyan
    e.send(Command::SetLayer { layer: 0, props: Layer { effects: vec![invert()], ..Layer::default() } });
    e.step(0.016);
    let c = centre(&pixels(&e));
    assert!(c[0] < 5 && c[1] > 250 && c[2] > 250, "layer invert {c:?}");

    // Clip effect on top of the layer effect: back to red
    let clip = Clip { effects: vec![invert()], ..stretch("red_64x36.png") };
    e.send(Command::UpdateClip { layer: 0, clip });
    e.step(0.016);
    let c = centre(&pixels(&e));
    assert!(c[0] > 250 && c[1] < 5, "clip + layer invert {c:?}");

    // Composition effect: once more → cyan; bypassed → red
    e.send(Command::SetCompositionEffects(vec![invert()]));
    e.step(0.016);
    assert!(centre(&pixels(&e))[0] < 5);
    e.send(Command::SetCompositionEffects(vec![evj_core::effect::EffectRef { bypass: true, ..invert() }]));
    e.step(0.016);
    assert!(centre(&pixels(&e))[0] > 250);

    // Unknown effects are skipped, not fatal.
    e.send(Command::SetCompositionEffects(vec![evj_core::effect::EffectRef::new("Does Not Exist")]));
    e.step(0.016);
    assert!(centre(&pixels(&e))[0] > 250);
    assert!(e.snapshot().effects.iter().any(|i| i.meta.name == "Invert"));
    e.shutdown().unwrap();
}


fn crossfade(secs: f64) -> evj_core::transition::TransitionPreset {
    use evj_core::transition::*;
    TransitionPreset { time: TransitionTime::Seconds(secs), easing: Easing::Linear, ..TransitionPreset::crossfade() }
}

#[test]
fn crossfade_between_clips_then_finishes() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
    step_until(&e, 0.016, showing(0));
    e.send(Command::Trigger { layer: 0, clip: stretch("blue_36x64.jpg"), transition: Some(crossfade(1.0)) });
    let s = step_until(&e, 0.0, |s| s.layers[0].transition.is_some());
    assert_eq!(s.layers[0].clip_name.as_deref(), Some("blue_36x64"));
    e.step(0.5);
    let c = centre(&pixels(&e));
    assert!((c[0] as i32 - 127).abs() < 10 && (c[2] as i32 - 127).abs() < 14, "half way {c:?}");
    e.step(0.6);
    let c = centre(&pixels(&e));
    assert!(c[0] < 10 && c[2] > 240, "done {c:?}");
    assert!(e.snapshot().layers[0].transition.is_none());
    e.shutdown().unwrap();
}

#[test]
fn clear_with_transition_fades_out() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
    step_until(&e, 0.016, showing(0));
    e.send(Command::Clear { layer: 0, transition: Some(crossfade(1.0)) });
    e.step(0.0);
    e.step(0.5);
    let c = centre(&pixels(&e));
    assert!((c[0] as i32 - 127).abs() < 10, "half faded {c:?}");
    e.step(0.6);
    assert!(centre(&pixels(&e))[0] < 5, "gone");
    assert!(!e.snapshot().layers[0].has_frame);
    e.shutdown().unwrap();
}

#[test]
fn cut_preset_and_unknown_shader_switch_instantly() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
    step_until(&e, 0.016, showing(0));
    let unknown = evj_core::transition::TransitionPreset { shader: "Nope".into(), ..crossfade(1.0) };
    e.send(Command::Trigger { layer: 0, clip: stretch("blue_36x64.jpg"), transition: Some(unknown) });
    step_until(&e, 0.0, |s| s.layers[0].clip_name.as_deref() == Some("blue_36x64"));
    e.step(0.016);
    assert!(centre(&pixels(&e))[2] > 240, "unknown shader = cut");
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: Some(evj_core::transition::TransitionPreset::cut()) });
    step_until(&e, 0.0, |s| s.layers[0].clip_name.as_deref() == Some("red_64x36"));
    e.step(0.016);
    assert!(centre(&pixels(&e))[0] > 240);
    e.shutdown().unwrap();
}

#[test]
fn transition_preview_renders_while_selected() {
    let e = engine();
    let (tx, rx) = channel();
    e.send(Command::ShareTransitionPreview(tx));
    let (_, w, h) = rx.recv().unwrap().unwrap();
    assert!(w > 0 && h > 0);
    e.send(Command::PreviewTransition(Some("Crossfade".into())));
    e.step(0.1);
    assert_eq!(e.snapshot().transition_preview.as_deref(), Some("Crossfade"));
    e.send(Command::PreviewTransition(Some("Wipe".into())));
    e.step(0.1);
    assert_eq!(e.snapshot().transition_preview.as_deref(), Some("Wipe"), "switching keeps previewing");
    e.send(Command::PreviewTransition(None));
    e.step(0.1);
    assert_eq!(e.snapshot().transition_preview, None);
    e.shutdown().unwrap();
}

/// Left half red, right half blue (24-bit BMP, bottom-up rows).
fn half_red_half_blue() -> PathBuf {
    let (w, h) = (16u32, 8u32);
    let row = (w * 3).div_ceil(4) * 4;
    let mut px = Vec::new();
    for _ in 0..h {
        for x in 0..w {
            px.extend_from_slice(if x < w / 2 { &[0, 0, 255] } else { &[255, 0, 0] });
        }
        px.resize(px.len() + (row - w * 3) as usize, 0);
    }
    let mut f = Vec::new();
    f.extend_from_slice(b"BM");
    f.extend_from_slice(&(54 + px.len() as u32).to_le_bytes());
    f.extend_from_slice(&[0, 0, 0, 0, 54, 0, 0, 0, 40, 0, 0, 0]);
    f.extend_from_slice(&w.to_le_bytes());
    f.extend_from_slice(&h.to_le_bytes());
    f.extend_from_slice(&[1, 0, 24, 0, 0, 0, 0, 0]);
    f.extend_from_slice(&(px.len() as u32).to_le_bytes());
    f.extend_from_slice(&[0; 16]);
    f.extend_from_slice(&px);
    let p = std::env::temp_dir().join(format!("evj-halves-{}.bmp", std::process::id()));
    std::fs::write(&p, f).unwrap();
    p
}

fn output_pixels(e: &Engine, config: evj_core::output::OutputConfig) -> Vec<u8> {
    let (tx, rx) = channel();
    e.send(Command::RenderOutputPreview { config, width: 8, height: 4, reply: tx });
    rx.recv().unwrap()
}

#[test]
fn output_slices_crop_the_composition() {
    use evj_core::output::{OutputConfig, Slice};
    let e = engine();
    let clip = Clip { fit: evj_core::model::FitMode::Stretch, ..Clip::new(half_red_half_blue()) };
    e.send(Command::Trigger { layer: 0, clip, transition: None });
    step_until(&e, 0.016, showing(0));
    e.step(0.016);
    let full = output_pixels(&e, OutputConfig::default());
    assert!(full[0] > 200 && full[(7 * 4) + 2] > 200, "full: red left, blue right");
    let right = Slice { name: "right".into(), input: [0.5, 0.0, 0.5, 1.0], output: [0.0, 0.0, 1.0, 1.0] };
    let px = output_pixels(&e, OutputConfig { slices: vec![right], ..OutputConfig::default() });
    assert!(px.chunks(4).all(|p| p[2] > 200 && p[0] < 60), "only the blue half, stretched");
    e.shutdown().unwrap();
}

#[test]
fn output_can_show_a_single_layer_or_a_test_pattern() {
    use evj_core::output::{OutputConfig, OutputSource};
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
    e.send(Command::Trigger { layer: 1, clip: stretch("blue_36x64.jpg"), transition: None });
    step_until(&e, 0.016, |s| s.layers[0].has_frame && s.layers[1].has_frame);
    e.step(0.016);
    let comp = output_pixels(&e, OutputConfig::default());
    assert!(comp[2] > 200, "composition: blue on top");
    let layer0 = output_pixels(&e, OutputConfig { source: OutputSource::Layer(0), ..OutputConfig::default() });
    e.step(0.016); // layer taps are captured while rendering
    let layer0 = if layer0[0] > 200 { layer0 } else { output_pixels(&e, OutputConfig { source: OutputSource::Layer(0), ..OutputConfig::default() }) };
    assert!(layer0[0] > 200 && layer0[2] < 60, "layer 1 alone is red: {:?}", &layer0[..4]);
    let tp = output_pixels(&e, OutputConfig { test_pattern: true, ..OutputConfig::default() });
    assert_ne!(tp, comp);
    e.shutdown().unwrap();
}

#[test]
fn cued_clip_shows_on_preview_only() {
    let e = engine();
    let (tx, rx) = channel();
    e.send(Command::ShareCuePreview(tx));
    e.step(0.0);
    let (_, w, h) = rx.recv().unwrap().unwrap();
    assert_eq!((w, h), (640, 320), "composition aspect (8:4)");
    e.send(Command::CueClip(Some(stretch("red_64x36.png"))));
    let s = step_until(&e, 0.016, |s| s.cue.is_some());
    assert_eq!(s.cue.as_deref(), Some("red_64x36"));
    let px = output_pixels(&e, evj_core::output::OutputConfig::default());
    assert!(px.chunks(4).all(|p| p[..3] == [0, 0, 0]), "the audience sees nothing of the cue");
    e.send(Command::CueClip(None));
    e.step(0.016);
    assert_eq!(e.snapshot().cue, None);
    e.shutdown().unwrap();
}

#[test]
fn layer_effects_blend_straight_onto_the_composition() {
    use evj_core::effect::EffectRef;
    use evj_core::model::BlendMode;
    // Red below; blue on top through an (identity) effect, 50 % Screen / Multiply: the effect's
    // pass draws straight onto the composition with a fixed-function blend.
    let mut identity = EffectRef::new("Invert");
    identity.param_mut("amount", 0.0).value = 0.0;
    for (blend, want) in [(BlendMode::Screen, [255, 0, 128]), (BlendMode::Multiply, [128, 0, 0]), (BlendMode::Alpha, [128, 0, 128])] {
        let e = engine();
        e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
        e.send(Command::Trigger { layer: 1, clip: stretch("blue_36x64.jpg"), transition: None });
        e.send(Command::SetLayer { layer: 1, props: Layer { blend, opacity: 0.5, effects: vec![identity.clone()], ..Layer::default() } });
        step_until(&e, 0.016, |s| s.layers[0].has_frame && s.layers[1].has_frame);
        e.step(0.016);
        let px = output_pixels(&e, evj_core::output::OutputConfig::default());
        for c in 0..3 {
            assert!((px[c] as i32 - want[c]).abs() <= 6, "{blend:?}: {:?} vs {want:?}", &px[..4]);
        }
        e.shutdown().unwrap();
    }
}

#[test]
fn clip_at_composition_size_skips_the_layer_copy_and_still_feeds_taps() {
    use evj_core::output::{OutputConfig, OutputSource};
    // Composition = the BMP's size, so the clip texture is used directly (no prepare pass).
    let e = Engine::start(EngineConfig { device: DeviceKind::Warp, manual_clock: true, width: 16, height: 8, layers: 2, effect_folders: vec![], audio: false }).unwrap();
    e.send(Command::Trigger { layer: 0, clip: Clip::new(half_red_half_blue()), transition: None });
    step_until(&e, 0.016, showing(0));
    e.step(0.016);
    let comp = output_pixels(&e, OutputConfig::default());
    assert!(comp[0] > 200 && comp[2] < 60 && comp[7 * 4 + 2] > 200, "red left, blue right: {:?}", &comp[..8]);
    let tap = || output_pixels(&e, OutputConfig { source: OutputSource::Layer(0), ..OutputConfig::default() });
    let _ = tap();
    e.step(0.016); // taps are captured while rendering
    let layer0 = tap();
    assert_eq!(layer0, comp, "the layer tap shows the same picture");
    e.shutdown().unwrap();
}

#[test]
fn recovers_from_device_loss() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
    e.send(Command::SetLayer { layer: 0, props: Layer { opacity: 0.5, ..Layer::default() } });
    step_until(&e, 0.016, showing(0));
    let (tx, rx) = channel();
    e.send(Command::SharePreview(tx));
    rx.recv().unwrap().unwrap();
    e.send(Command::SimulateDeviceLost);
    let s = step_until(&e, 0.016, |s| s.device_resets == 1 && s.layers[0].has_frame);
    assert!(!s.preview, "UI must re-open the preview on the new device");
    e.step(0.016);
    let c = centre(&pixels(&e));
    assert!((c[0] as i32 - 127).abs() < 10, "clip and layer settings survive: {c:?}");
    e.shutdown().unwrap();
}

#[test]
fn snapshot_reports_recent_frame_times() {
    let e = Engine::start(EngineConfig { device: DeviceKind::Warp, manual_clock: false, width: 8, height: 4, layers: 1, effect_folders: vec![], audio: false }).unwrap();
    std::thread::sleep(std::time::Duration::from_millis(400));
    let s = e.snapshot();
    assert!(s.frame_times.len() >= 5 && s.frame_times.len() <= 120, "{}", s.frame_times.len());
    assert!(s.frame_times.iter().all(|t| *t > 0.0 && *t < 1000.0));
    e.shutdown().unwrap();
}


fn audio_engine() -> Option<Engine> {
    let e = Engine::start(EngineConfig { device: DeviceKind::Warp, manual_clock: true, width: 8, height: 4, layers: 2, effect_folders: vec![], audio: true }).ok()?;
    e.step(0.0);
    if e.snapshot().audio_device.is_none() {
        eprintln!("no audio device: skipped");
        return None;
    }
    e.send(Command::SetMasterVolume(0.02)); // quiet test
    Some(e)
}

#[test]
fn video_clock_follows_clip_audio() {
    let Some(e) = audio_engine() else { return };
    e.send(Command::Trigger { layer: 0, clip: Clip::new(fixture("av_320x240.mp4")), transition: None });
    // The manual clock does not move (dt = 0): only the audio clock can advance the clip.
    let s = step_until(&e, 0.0, |s| s.layers[0].audio && s.layers[0].pos > 0.2);
    assert!(s.layers[0].has_frame);
    e.send(Command::Clear { layer: 0, transition: None });
    step_until(&e, 0.0, |s| !s.layers[0].audio);
    e.shutdown().unwrap();
}

#[test]
fn audio_only_clip_plays_without_picture() {
    let Some(e) = audio_engine() else { return };
    e.send(Command::Trigger { layer: 1, clip: Clip::new(fixture("tone_44k_mono.mp3")), transition: None });
    let s = step_until(&e, 0.0, |s| s.layers[1].audio && s.layers[1].pos > 0.1);
    assert_eq!(s.layers[1].clip_name.as_deref(), Some("tone_44k_mono"));
    assert!(!s.layers[1].has_frame);
    assert_eq!(s.layers[1].kind, Some(evj_engine::DecoderKind::Audio));
    e.shutdown().unwrap();
}

/// Deck: slide 1 red image, slide 2 a 1 s video with a click at 0.5 s, slide 3 blue image.
fn test_deck() -> PathBuf {
    use evj_core::slides::{Slide, SlideDeck};
    let dir = std::env::temp_dir().join(format!("evj-deck-engine-{}-{:?}", std::process::id(), std::thread::current().id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let img = |name: &str, f: &str| {
        let p = dir.join(name);
        std::fs::copy(fixture(f), &p).unwrap();
        p
    };
    let s1 = Slide { image: img("s1.png", "red_64x36.png"), video: None, steps: vec![0.0], end: 0.0, notes: "one".into() };
    let s2 = Slide { image: img("s2.png", "red_64x36.png"), video: Some(img("s2.mp4", "h264_320x240.mp4")), steps: vec![0.0, 0.5], end: 1.0, notes: "two".into() };
    let s3 = Slide { image: img("s3.jpg", "blue_36x64.jpg"), video: None, steps: vec![0.0], end: 0.0, notes: "three".into() };
    let d = SlideDeck { title: "t".into(), source: dir.join("t.pptx"), width: 64, height: 36, slides: vec![s1, s2, s3] };
    d.save(&dir).unwrap();
    dir.join(evj_core::slides::MANIFEST)
}

#[test]
fn slides_step_through_images_and_click_animations() {
    let e = engine();
    let deck = Clip { fit: evj_core::model::FitMode::Stretch, ..Clip::new(test_deck()) };
    e.send(Command::Trigger { layer: 0, clip: deck, transition: None });
    let s = step_until(&e, 0.016, showing(0));
    assert_eq!(s.layers[0].slide, Some((0, 0, 3)));
    assert!(centre(&pixels(&e))[0] > 240, "slide 1 red");

    e.send(Command::SlideNext { layer: 0, transition: None });
    let s = step_until(&e, 0.016, |s| s.layers[0].slide == Some((1, 0, 3)) && s.layers[0].has_frame && !s.layers[0].loading);
    // The animation video plays up to the first click boundary (0.5 s) and waits there.
    for _ in 0..60 {
        e.step(0.05);
    }
    let pos = e.snapshot().layers[0].pos;
    assert!(pos <= 0.55 && pos > 0.3, "paused at the click: {pos} ({:?})", s.layers[0].slide);
    e.send(Command::SlideNext { layer: 0, transition: None });
    e.step(0.0);
    assert_eq!(e.snapshot().layers[0].slide, Some((1, 1, 3)));
    for _ in 0..20 {
        e.step(0.05);
    }
    assert!(e.snapshot().layers[0].pos > 0.7, "continued after the click");

    e.send(Command::SlideNext { layer: 0, transition: None });
    step_until(&e, 0.016, |s| s.layers[0].slide == Some((2, 0, 3)) && s.layers[0].clip_name.as_deref() == Some("s3"));
    e.step(0.016);
    assert!(centre(&pixels(&e))[2] > 240, "slide 3 blue");

    e.send(Command::SlideNext { layer: 0, transition: None });
    e.step(0.0);
    assert_eq!(e.snapshot().layers[0].slide, Some((2, 0, 3)), "stays on the last slide");

    e.send(Command::SlidePrev { layer: 0, transition: None });
    step_until(&e, 0.016, |s| s.layers[0].slide == Some((1, 1, 3)) && s.layers[0].clip_name.as_deref() == Some("s2"));
    e.send(Command::SlideGoto { layer: 0, slide: 0, transition: None });
    step_until(&e, 0.016, |s| s.layers[0].slide == Some((0, 0, 3)) && s.layers[0].clip_name.as_deref() == Some("s1"));
    e.shutdown().unwrap();
}

#[test]
fn pointer_dot_and_spotlight_on_a_layer() {
    use evj_engine::Pointer;
    let e = Engine::start(EngineConfig { device: DeviceKind::Warp, manual_clock: true, width: 64, height: 32, layers: 1, effect_folders: vec![], audio: false }).unwrap();
    e.send(Command::Trigger { layer: 0, clip: stretch("blue_36x64.jpg"), transition: None });
    step_until(&e, 0.016, showing(0));
    let at = |px: &[u8], x: usize, y: usize| [px[(y * 64 + x) * 4], px[(y * 64 + x) * 4 + 1], px[(y * 64 + x) * 4 + 2]];

    e.send(Command::SetPointer { layer: 0, pointer: Some(Pointer { x: 0.5, y: 0.5, spotlight: false, size: 0.1 }) });
    e.step(0.016);
    let px = pixels(&e);
    assert!(at(&px, 32, 16)[0] > 200, "red laser dot in the middle: {:?}", at(&px, 32, 16));
    assert!(at(&px, 2, 2)[2] > 200 && at(&px, 2, 2)[0] < 40, "corner untouched: {:?}", at(&px, 2, 2));

    e.send(Command::SetPointer { layer: 0, pointer: Some(Pointer { x: 0.5, y: 0.5, spotlight: true, size: 0.2 }) });
    e.step(0.016);
    let px = pixels(&e);
    assert!(at(&px, 2, 2)[2] < at(&px, 32, 16)[2] - 60, "spotlight dims the rest");

    e.send(Command::SetPointer { layer: 0, pointer: None });
    e.step(0.016);
    assert!(at(&pixels(&e), 32, 16)[0] < 40, "pointer off");
    e.shutdown().unwrap();
}

#[test]
fn remaining_time_counts_down() {
    let e = engine();
    let clip = Clip { mode: evj_core::model::PlayMode::Once, ..Clip::new(fixture("hap1_64x48.mov")) };
    e.send(Command::Trigger { layer: 0, clip, transition: None });
    let s = step_until(&e, 0.016, showing(0));
    let r0 = s.layers[0].remaining.unwrap();
    e.step(0.05);
    let r1 = e.snapshot().layers[0].remaining.unwrap();
    assert!(r1 < r0 && r0 - r1 < 0.2, "{r0} -> {r1}");
    assert!(!e.snapshot().layers[0].looping);
    e.shutdown().unwrap();
}

#[test]
fn blackout_fades_the_output_and_back() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
    step_until(&e, 0.016, showing(0));
    e.send(Command::Blackout(true));
    for _ in 0..40 {
        e.step(0.016);
    }
    assert_eq!(e.snapshot().blackout, 1.0);
    let px = output_pixels(&e, evj_core::output::OutputConfig::default());
    assert!(px.chunks(4).all(|p| p[0] < 3), "black: {:?}", &px[..4]);
    e.send(Command::Blackout(false));
    for _ in 0..40 {
        e.step(0.016);
    }
    let px = output_pixels(&e, evj_core::output::OutputConfig::default());
    assert!(px[0] > 240, "red again");
    e.shutdown().unwrap();
}

#[test]
fn cue_pauses_and_rewinds() {
    let e = engine();
    let (tx, rx) = channel();
    e.send(Command::ShareCuePreview(tx));
    e.step(0.0);
    rx.recv().unwrap().unwrap();
    e.send(Command::CueClip(Some(Clip::new(fixture("hap1_64x48.mov")))));
    step_until(&e, 0.016, |s| s.cue_state.is_some());
    e.send(Command::CuePause(true));
    e.step(0.0);
    let p0 = e.snapshot().cue_state.unwrap().pos;
    e.step(0.1);
    let st = e.snapshot().cue_state.unwrap();
    assert!(st.paused && (st.pos - p0).abs() < 1e-9, "paused holds the position");
    e.send(Command::CuePause(false));
    e.step(0.05);
    e.send(Command::CueRewind);
    step_until(&e, 0.016, |s| s.cue_state.as_ref().is_some_and(|c| c.paused && c.pos < 0.02));
    e.shutdown().unwrap();
}

#[test]
fn cued_h264_video_reaches_the_preview() {
    let e = engine();
    let (tx, rx) = channel();
    e.send(Command::ShareCuePreview(tx));
    e.step(0.0);
    rx.recv().unwrap().unwrap();
    e.send(Command::CueClip(Some(Clip::new(fixture("av_320x240.mp4")))));
    let s = step_until(&e, 0.016, |s| s.cue_state.is_some());
    assert_eq!(s.cue.as_deref(), Some("av_320x240"));
    e.shutdown().unwrap();
}

#[test]
fn blackout_also_darkens_single_layer_outputs() {
    use evj_core::output::{OutputConfig, OutputSource};
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
    step_until(&e, 0.016, showing(0));
    let tap = || output_pixels(&e, OutputConfig { source: OutputSource::Layer(0), ..OutputConfig::default() });
    let _ = tap();
    e.step(0.016);
    assert!(tap()[0] > 240, "layer tap shows red before the blackout");
    e.send(Command::Blackout(true));
    for _ in 0..40 {
        e.step(0.016);
    }
    let px = tap();
    assert!(px.chunks(4).all(|p| p[0] < 3), "tap black too: {:?}", &px[..4]);
    e.shutdown().unwrap();
}

fn cue_engine() -> Engine {
    let e = engine();
    let (tx, rx) = channel();
    e.send(Command::ShareCuePreview(tx));
    e.step(0.0);
    rx.recv().unwrap().unwrap();
    e
}

#[test]
fn seek_moves_a_random_access_clip_even_when_paused() {
    let e = cue_engine();
    e.send(Command::CueClip(Some(Clip::new(fixture("hap1_64x48.mov")))));
    step_until(&e, 0.0, |s| s.cue_state.is_some());
    e.send(Command::CuePause(true));
    e.send(Command::Seek { layer: None, secs: 0.07 });
    let s = step_until(&e, 0.0, |s| s.cue_state.as_ref().is_some_and(|c| (c.pos - 0.07).abs() < 1e-6));
    assert!(s.cue_state.unwrap().paused, "still paused after the seek");
    e.send(Command::Seek { layer: None, secs: 5.0 });
    step_until(&e, 0.0, |s| s.cue_state.as_ref().is_some_and(|c| c.pos <= c.end + 1e-9 && c.pos > 0.08));
    e.shutdown().unwrap();
}

#[test]
fn seek_reopens_a_paused_h264_clip_at_the_new_time() {
    let e = cue_engine();
    e.send(Command::CueClip(Some(Clip::new(fixture("h264_320x240.mp4")))));
    step_until(&e, 0.016, |s| s.cue_state.is_some());
    e.send(Command::CuePause(true));
    e.step(0.0);
    e.send(Command::Seek { layer: None, secs: 0.6 });
    let s = step_until(&e, 0.016, |s| s.cue_state.as_ref().is_some_and(|c| c.pos >= 0.55));
    assert!(s.cue_state.unwrap().paused);
    e.shutdown().unwrap();
}

#[test]
fn seek_on_a_program_layer() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: Clip { mode: PlayMode::PingPong, ..Clip::new(fixture("hap1_64x48.mov")) }, transition: None });
    step_until(&e, 0.0, showing(0));
    e.send(Command::Seek { layer: Some(0), secs: 0.05 });
    step_until(&e, 0.0, |s| (s.layers[0].pos - 0.05).abs() < 1e-6);
    e.shutdown().unwrap();
}

#[test]
fn h264_in_and_out_points_are_honoured() {
    let e = engine();
    let clip = Clip { mode: PlayMode::Once, in_point: 0.5, out_point: 0.8, ..Clip::new(fixture("h264_320x240.mp4")) };
    e.send(Command::Trigger { layer: 0, clip, transition: None });
    let s = step_until(&e, 0.0, showing(0));
    assert!(s.layers[0].pos >= 0.45, "starts at the in point: {}", s.layers[0].pos);
    assert!((s.layers[0].start - 0.5).abs() < 0.06 && (s.layers[0].end - 0.8).abs() < 0.06, "{s:?}");
    let s = step_until(&e, 0.02, |s| s.layers[0].finished);
    assert!(s.layers[0].pos <= 0.85, "stops at the out point: {}", s.layers[0].pos);
    e.shutdown().unwrap();
}

#[test]
fn still_image_with_a_duration_finishes() {
    let e = engine();
    let clip = Clip { still_secs: Some(0.1), ..stretch("red_64x36.png") };
    e.send(Command::Trigger { layer: 0, clip, transition: None });
    let s = step_until(&e, 0.0, showing(0));
    assert!(!s.layers[0].finished);
    assert!(s.layers[0].remaining.is_some_and(|r| r <= 0.1));
    step_until(&e, 0.02, |s| s.layers[0].finished);
    e.shutdown().unwrap();
}

#[test]
fn queued_clip_follows_without_a_gap() {
    let e = engine();
    let first = Clip { mode: PlayMode::Once, ..Clip::new(fixture("hap1_64x48.mov")) };
    e.send(Command::Trigger { layer: 0, clip: first, transition: None });
    step_until(&e, 0.0, showing(0));
    e.send(Command::QueueNext { layer: 0, clip: Some(Clip { still_secs: Some(1.0), ..stretch("red_64x36.png") }) });
    for _ in 0..200 {
        e.step(0.01);
        let s = e.snapshot();
        assert!(s.layers[0].has_frame, "never empty between the clips");
        if s.layers[0].queue_taken == 1 && s.layers[0].clip_name.as_deref() == Some("red_64x36") {
            e.shutdown().unwrap();
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    panic!("the queued clip never came: {:?}", e.snapshot());
}

#[test]
fn trigger_drops_the_queue_and_a_missing_queued_file_keeps_the_current_clip() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: Clip { mode: PlayMode::Once, ..Clip::new(fixture("hap1_64x48.mov")) }, transition: None });
    step_until(&e, 0.0, showing(0));
    e.send(Command::QueueNext { layer: 0, clip: Some(Clip::new(PathBuf::from("C:/nope/gone.mov"))) });
    let s = step_until(&e, 0.02, |s| s.layers[0].error.is_some());
    assert!(s.layers[0].has_frame, "the last picture stays");
    assert_eq!((s.layers[0].queue_taken, s.layers[0].queue_failed), (0, 1), "a failed open is not a swap");
    e.send(Command::QueueNext { layer: 0, clip: Some(stretch("blue_36x64.jpg")) });
    e.send(Command::Trigger { layer: 0, clip: stretch("red_64x36.png"), transition: None });
    step_until(&e, 0.02, |s| s.layers[0].clip_name.as_deref() == Some("red_64x36"));
    for _ in 0..20 {
        e.step(0.02);
    }
    assert_eq!(e.snapshot().layers[0].clip_name.as_deref(), Some("red_64x36"), "the queue went with the trigger");
    e.shutdown().unwrap();
}

fn with_music(mut clip: Clip, mix: bool) -> Clip {
    clip.attached = Some(evj_core::model::AttachedAudio { path: fixture("tone_44k_mono.mp3"), mix, volume: 1.0 });
    clip
}

#[test]
fn image_with_attached_audio_lasts_as_long_as_the_audio() {
    let e = engine(); // no audio device needed: the length is probed when the clip opens
    let clip = Clip { mode: PlayMode::Once, ..with_music(stretch("red_64x36.png"), false) };
    e.send(Command::Trigger { layer: 0, clip, transition: None });
    let s = step_until(&e, 0.0, showing(0));
    assert!(s.layers[0].duration > 0.5, "duration from the audio: {}", s.layers[0].duration);
    assert!(s.layers[0].remaining.is_some_and(|r| r > 0.5));
    assert!((s.layers[0].end - s.layers[0].duration).abs() < 1e-9);
    e.shutdown().unwrap();
}

#[test]
fn attached_audio_plays_with_the_picture() {
    let Some(e) = audio_engine() else { return };
    e.send(Command::Trigger { layer: 0, clip: with_music(stretch("red_64x36.png"), false), transition: None });
    let s = step_until(&e, 0.0, |s| s.layers[0].audio && s.layers[0].pos > 0.1);
    assert!(s.layers[0].has_frame);
    e.send(Command::Clear { layer: 0, transition: None });
    step_until(&e, 0.0, |s| !s.layers[0].audio);
    e.shutdown().unwrap();
}

#[test]
fn replace_silences_the_clips_own_sound_and_mix_keeps_it() {
    let Some(e) = audio_engine() else { return };
    e.send(Command::Trigger { layer: 0, clip: with_music(Clip::new(fixture("av_320x240.mp4")), false), transition: None });
    let s = step_until(&e, 0.016, |s| s.layers[0].audio);
    assert_eq!(s.layers[0].voices, 1, "only the attached voice");
    e.send(Command::Trigger { layer: 1, clip: with_music(Clip::new(fixture("av_320x240.mp4")), true), transition: None });
    step_until(&e, 0.016, |s| s.layers[1].voices == 2);
    e.shutdown().unwrap();
}

fn cue_pixels(e: &Engine) -> Vec<u8> {
    let (tx, rx) = channel();
    e.send(Command::ReadbackCue(tx));
    e.step(0.0);
    rx.recv().unwrap()
}

#[test]
fn scene_preview_blends_the_layers() {
    let e = cue_engine();
    e.send(Command::SetLayer { layer: 1, props: Layer { opacity: 0.5, ..Layer::default() } });
    e.send(Command::CueScene(vec![Some(stretch("blue_36x64.jpg")), Some(stretch("red_64x36.png")), None, None]));
    let s = step_until(&e, 0.016, |s| s.cue.is_some() && s.cue_layers == 2);
    assert!(s.layers.iter().all(|l| l.clip_name.is_none()), "nothing on Program");
    e.step(0.016);
    let px = cue_pixels(&e);
    let mid = (160 * 640 + 320) * 4; // RGBA
    let (r, b) = (px[mid] as i32, px[mid + 2] as i32);
    assert!((r - 127).abs() < 15 && (b - 127).abs() < 15, "half red over blue: r {r} b {b}");
    e.send(Command::CueClip(Some(stretch("red_64x36.png"))));
    step_until(&e, 0.016, |s| s.cue_layers == 1);
    e.shutdown().unwrap();
}


#[test]
fn a_presentation_reports_its_deck_file() {
    let e = engine();
    e.send(Command::Trigger { layer: 0, clip: Clip::new(test_deck()), transition: None });
    let s = step_until(&e, 0.016, |s| s.layers[0].slide.is_some() && s.layers[0].has_frame);
    let p = s.layers[0].clip_path.clone().unwrap();
    assert!(evj_core::slides::SlideDeck::is_deck(&p), "clip_path is the deck, not a slide: {p:?}");
    e.shutdown().unwrap();
}

#[test]
fn a_loop_with_only_a_start_point_wraps_to_it() {
    let e = engine();
    let clip = Clip { mode: PlayMode::Loop, in_point: 0.5, ..Clip::new(fixture("h264_320x240.mp4")) };
    e.send(Command::Trigger { layer: 0, clip, transition: None });
    step_until(&e, 0.0, showing(0));
    let mut wrapped = false;
    let mut last = 0.0;
    for _ in 0..400 {
        e.step(0.01);
        let s = e.snapshot();
        let p = s.layers[0].pos;
        if s.layers[0].has_frame && p + 0.2 < last {
            wrapped = true;
            assert!(p >= 0.4, "wrapped to the start point, not the file start: {p}");
        }
        last = p;
        std::thread::sleep(std::time::Duration::from_millis(2));
        if wrapped {
            break;
        }
    }
    assert!(wrapped, "the A–B loop wrapped");
    e.shutdown().unwrap();
}

#[test]
fn a_trimmed_h264_loop_is_preloaded_so_the_wrap_is_quick() {
    let e = engine();
    let clip = Clip { mode: PlayMode::Loop, in_point: 0.2, out_point: 0.6, ..Clip::new(fixture("h264_320x240.mp4")) };
    e.send(Command::Trigger { layer: 0, clip, transition: None });
    step_until(&e, 0.0, showing(0));
    // Give the preload time to open, then run to the out point.
    std::thread::sleep(std::time::Duration::from_millis(500));
    let mut at_end = None;
    for i in 0..400 {
        e.step(0.02);
        let p = e.snapshot().layers[0].pos;
        if at_end.is_none() && p >= 0.58 {
            at_end = Some(i);
        }
        if let Some(t) = at_end.filter(|_| p < 0.3) {
            assert!(i - t <= 6, "back at the start within a few frames, took {}", i - t);
            e.shutdown().unwrap();
            return;
        }
    }
    panic!("never wrapped");
}

/// No Program audio, Preview on the default device (no clash): runs on a one-device laptop.
fn preview_only_engine() -> Option<Engine> {
    evj_audio::AudioEngine::default_device()?;
    let e = engine(); // audio: false → no Program output
    let (tx, rx) = channel();
    e.send(Command::ShareCuePreview(tx));
    e.send(Command::SetAudioRoute { bus: evj_engine::Bus::Preview, route: Some(evj_audio::Route::default()) });
    e.send(Command::SetPreviewVolume(0.05));
    e.step(0.0);
    rx.recv().unwrap().unwrap();
    Some(e)
}

#[test]
fn the_cued_clip_sounds_on_preview() {
    let Some(e) = preview_only_engine() else { return };
    e.send(Command::CueClip(Some(Clip::new(fixture("tone_44k_mono.mp3")))));
    let s = step_until(&e, 0.016, |s| s.preview_peaks[0] > 0.0);
    assert!(s.preview_audio_device.is_some() && s.preview_audio_error.is_none());
    e.send(Command::CuePause(true));
    step_until(&e, 0.016, |s| s.preview_peaks == [0.0, 0.0]);
    e.send(Command::CuePause(false));
    step_until(&e, 0.016, |s| s.preview_peaks[0] > 0.0);
    // A scene: every cued layer sounds.
    e.send(Command::CueScene(vec![Some(Clip::new(fixture("tone_44k_mono.mp3"))), Some(Clip::new(fixture("av_320x240.mp4"))), None, None]));
    step_until(&e, 0.016, |s| s.cue_layers == 2 && s.preview_peaks[0] > 0.0);
    e.send(Command::CueClip(None));
    step_until(&e, 0.016, |s| s.preview_peaks == [0.0, 0.0]);
    e.shutdown().unwrap();
}

#[test]
fn a_missing_preview_device_reports_and_program_plays_on() {
    let Some(e) = audio_engine() else { return };
    e.send(Command::SetAudioRoute { bus: evj_engine::Bus::Preview, route: Some(evj_audio::Route { device: Some("No Such Headphones 123".into()), first_channel: 0 }) });
    e.send(Command::Trigger { layer: 1, clip: Clip::new(fixture("tone_44k_mono.mp3")), transition: None });
    let s = step_until(&e, 0.0, |s| s.layers[1].audio && s.preview_audio_error.is_some());
    assert!(s.preview_audio_device.is_none());
    e.shutdown().unwrap();
}

#[test]
fn a_preview_route_on_the_program_pair_is_refused() {
    let Some(e) = audio_engine() else { return };
    e.send(Command::SetAudioRoute { bus: evj_engine::Bus::Preview, route: Some(evj_audio::Route { device: None, first_channel: 0 }) });
    let s = step_until(&e, 0.0, |s| s.preview_audio_error.is_some());
    assert!(s.preview_audio_error.unwrap().contains("Program"), "clash explained");
    e.shutdown().unwrap();
}

#[test]
fn changing_the_program_device_restarts_attached_audio() {
    let Some(e) = audio_engine() else { return };
    e.send(Command::Trigger { layer: 0, clip: with_music(stretch("red_64x36.png"), false), transition: None });
    step_until(&e, 0.0, |s| s.layers[0].voices == 1);
    e.send(Command::SetAudioRoute { bus: evj_engine::Bus::Program, route: None });
    for _ in 0..5 {
        e.step(0.0);
    }
    step_until(&e, 0.0, |s| s.layers[0].voices == 1 && s.layers[0].audio_peaks[0] > 0.0);
    e.shutdown().unwrap();
}
