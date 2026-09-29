//! Effect chain editor (clip / layer / composition) and the BPM widget.
use evj_core::effect::{EffectRef, Kind};
use evj_core::lfo::{Lfo, Wave};
use evj_engine::fx::EffectInfo;
use evj_engine::{Command, Snapshot};

/// What the effect editor reports besides "changed": an eyedropper request (index of the effect
/// whose Pick was pressed) and where its widgets are (layout checks, tests).
#[derive(Default)]
pub struct ChainOut {
    pub pick: Option<usize>,
    pub rects: Vec<(String, egui::Rect)>,
}

/// The three parameters that make up a key colour (Chroma Key, or any effect using them).
const KEY: [&str; 3] = ["key_r", "key_g", "key_b"];

/// A key colour (0..1, as the picture's pixels: sRGB) as the swatch's bytes, and back.
fn key_bytes(v: [f64; 3]) -> [u8; 3] {
    v.map(|c| (c.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn key_values(b: [u8; 3]) -> [f64; 3] {
    b.map(|c| c as f64 / 255.0)
}

/// Edits `chain` in place; true when anything changed. `eyedropper`: effects with a key colour
/// get a Pick button (clip effects: the clip is on a monitor).
pub fn chain(ui: &mut egui::Ui, salt: &str, chain: &mut Vec<EffectRef>, lib: &[EffectInfo], eyedropper: bool, out: &mut ChainOut) -> bool {
    let mut changed = false;
    let mut action: Option<(usize, i32)> = None; // (index, -1 up / +1 down / 0 remove)
    let n = chain.len();
    for (i, e) in chain.iter_mut().enumerate() {
        let info = lib.iter().find(|l| l.meta.name == e.name);
        let title = match info {
            Some(_) => e.name.clone(),
            None => format!("{} (missing)", e.name),
        };
        let id = ui.make_persistent_id((salt, i, &e.name));
        let (toggle, _, _) = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
            .show_header(ui, |ui| {
                changed |= ui.checkbox(&mut e.bypass, "").on_hover_text("Bypass").changed();
                ui.label(egui::RichText::new(title).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("✖").clicked() {
                        action = Some((i, 0));
                    }
                    if i + 1 < n && ui.small_button("⏷").on_hover_text("Move down").clicked() {
                        action = Some((i, 1));
                    }
                    if i > 0 && ui.small_button("⏶").on_hover_text("Move up").clicked() {
                        action = Some((i, -1));
                    }
                });
            })
            .body(|ui| {
                let Some(info) = info else {
                    ui.label("This effect is not in the library (renamed or deleted).");
                    return;
                };
                let keyed = KEY.iter().all(|k| info.meta.params.iter().any(|p| p.name == *k));
                if keyed {
                    ui.horizontal(|ui| {
                        ui.label("key colour");
                        let def = |k: &str| info.meta.params.iter().find(|p| p.name == k).map_or(0.0, |p| p.default as f64);
                        // sRGB bytes: the swatch shows the key exactly as the picture's pixels.
                        let mut rgb = key_bytes(KEY.map(|k| e.param_mut(k, def(k)).value));
                        if ui.color_edit_button_srgb(&mut rgb).changed() {
                            for (k, v) in KEY.iter().zip(key_values(rgb)) {
                                e.param_mut(k, def(k)).value = v;
                            }
                            changed = true;
                        }
                        if eyedropper {
                            let b = ui.button("Pick").on_hover_text("Then click the colour on the Preview or Program monitor (Esc cancels)");
                            out.rects.push(("Pick".into(), b.rect));
                            if b.clicked() {
                                out.pick = Some(i);
                            }
                        }
                    });
                }
                for d in info.meta.params.iter().filter(|d| !(keyed && KEY.contains(&d.name.as_str()))) {
                    let p = e.param_mut(&d.name, d.default as f64);
                    ui.horizontal(|ui| {
                        ui.label(&d.name);
                        let mut lfo_on = p.lfo.is_some();
                        ui.add_enabled_ui(!lfo_on, |ui| {
                            changed |= ui.add(egui::Slider::new(&mut p.value, d.min as f64..=d.max as f64)).changed();
                        });
                        if ui.toggle_value(&mut lfo_on, "LFO").changed() {
                            p.lfo = lfo_on.then(Lfo::default);
                            changed = true;
                        }
                    });
                    if let Some(l) = p.lfo.as_mut() {
                        ui.horizontal(|ui| {
                            ui.add_space(16.0);
                            egui::ComboBox::from_id_salt((salt, i, &d.name, "wave")).width(80.0).selected_text(format!("{:?}", l.wave)).show_ui(ui, |ui| {
                                for w in [Wave::Sine, Wave::Saw, Wave::Square, Wave::Triangle, Wave::Random] {
                                    changed |= ui.selectable_value(&mut l.wave, w, format!("{w:?}")).changed();
                                }
                            });
                            egui::ComboBox::from_id_salt((salt, i, &d.name, "beats")).width(70.0).selected_text(beats_label(l.beats)).show_ui(ui, |ui| {
                                for b in [0.25, 0.5, 1.0, 2.0, 4.0, 8.0, 16.0] {
                                    changed |= ui.selectable_value(&mut l.beats, b, beats_label(b)).changed();
                                }
                            });
                            changed |= ui.add(egui::DragValue::new(&mut l.min).range(0.0..=1.0).speed(0.01).prefix("min ")).changed();
                            changed |= ui.add(egui::DragValue::new(&mut l.max).range(0.0..=1.0).speed(0.01).prefix("max ")).changed();
                        });
                    }
                }
            });
        out.rects.push((e.name.clone(), toggle.rect));
    }
    if let Some((i, dir)) = action {
        match dir {
            0 => {
                chain.remove(i);
            }
            -1 => chain.swap(i, i - 1),
            _ => chain.swap(i, i + 1),
        }
        changed = true;
    }
    egui::ComboBox::from_id_salt((salt, "add")).selected_text("+ Add effect").show_ui(ui, |ui| {
        for info in lib.iter().filter(|l| l.meta.kind == Kind::Effect && l.error.is_none() && !l.meta.hidden) {
            if ui.selectable_label(false, &info.meta.name).clicked() {
                chain.push(EffectRef::new(&info.meta.name));
                changed = true;
            }
        }
    });
    changed
}

fn beats_label(b: f64) -> String {
    if b < 1.0 { format!("1/{} beat", (1.0 / b).round()) } else { format!("{b} beats") }
}

/// Custom effects that failed to load, with the compiler message.
pub fn errors(ui: &mut egui::Ui, lib: &[EffectInfo]) {
    for info in lib.iter().filter(|l| l.error.is_some()) {
        ui.colored_label(egui::Color32::LIGHT_RED, format!("{}: {}", info.meta.name, info.error.as_deref().unwrap_or("")));
    }
}

/// BPM value, tap, nudge, resync and a 4-beat indicator. Returns the new BPM if it was edited.
pub fn bpm(ui: &mut egui::Ui, snap: &Snapshot, commands: &mut Vec<Command>) -> Option<f64> {
    let mut edited = None;
    let mut bpm = snap.bpm;
    if ui.add(egui::DragValue::new(&mut bpm).range(20.0..=400.0).speed(0.1).max_decimals(1).suffix(" BPM")).changed() {
        commands.push(Command::SetBpm(bpm));
        edited = Some(bpm);
    }
    if ui.button("TAP").clicked() {
        commands.push(Command::Tap);
    }
    if ui.small_button("−").on_hover_text("Nudge back").clicked() {
        commands.push(Command::Nudge(-0.05));
    }
    if ui.small_button("+").on_hover_text("Nudge forward").clicked() {
        commands.push(Command::Nudge(0.05));
    }
    if ui.small_button("1").on_hover_text("Resync: now is beat one").clicked() {
        commands.push(Command::Resync);
    }
    let beat = snap.beat.rem_euclid(4.0).floor() as usize;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(52.0, 14.0), egui::Sense::hover());
    for i in 0..4 {
        let c = egui::pos2(rect.min.x + 6.0 + i as f32 * 13.0, rect.center().y);
        let on = i == beat;
        let color = if on { egui::Color32::from_rgb(255, 190, 60) } else { egui::Color32::from_gray(70) };
        ui.painter().circle_filled(c, if on { 5.0 } else { 4.0 }, color);
    }
    edited
}

#[cfg(test)]
mod tests {
    use super::{key_bytes, key_values};

    #[test]
    fn the_swatch_shows_the_key_as_the_pictures_pixels() {
        assert_eq!(key_bytes([0.0, 0.69, 0.25]), [0, 176, 64], "no linear / sRGB conversion");
        assert_eq!(key_bytes([1.2, -0.1, 1.0]), [255, 0, 255]);
        let back = key_values([0, 176, 64]);
        assert!((back[1] - 176.0 / 255.0).abs() < 1e-12);
    }
}
