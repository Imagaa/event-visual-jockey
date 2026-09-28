//! Output Manager (screens, sources, slices, test pattern) and the Performance window.
use crate::ui::{Actions, UiState};
use evj_core::output::{OutputConfig, OutputSource, Slice};
use evj_engine::Snapshot;
use egui::{Color32, Stroke, vec2};

/// Per-output preview textures (refreshed by the app from `RenderOutputPreview`).
pub type OutputPreviews = Vec<Option<egui::TextureHandle>>;

fn rect_editor(ui: &mut egui::Ui, label: &str, r: &mut [f32; 4]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
        for (v, name) in r.iter_mut().zip(["x", "y", "w", "h"]) {
            changed |= ui.add(egui::DragValue::new(v).range(0.0..=1.0).speed(0.005).max_decimals(3).prefix(format!("{name} "))).changed();
        }
    });
    changed
}

/// Draws the slices as boxes on a small canvas (input = where they come from).
fn slice_map(ui: &mut egui::Ui, slices: &[Slice], input: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(160.0, 90.0), egui::Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, Color32::from_gray(25));
    for (i, s) in slices.iter().enumerate() {
        let r = if input { s.input } else { s.output };
        let min = rect.min + vec2(r[0] * rect.width(), r[1] * rect.height());
        let b = egui::Rect::from_min_size(min, vec2(r[2] * rect.width(), r[3] * rect.height()));
        let c = Color32::from_rgb(80 + (i as u8 * 60) % 170, 160, 230 - (i as u8 * 50) % 150);
        p.rect_stroke(b, 1.0, Stroke::new(1.5, c), egui::StrokeKind::Inside);
        p.text(b.center(), egui::Align2::CENTER_CENTER, (i + 1).to_string(), egui::FontId::proportional(11.0), c);
    }
}

/// The Outputs panel (dock tab).
pub fn manager_body(ui: &mut egui::Ui, st: &mut UiState, act: &mut Actions, previews: &OutputPreviews) {
    let layers = st.project.composition.layers.len();
    let monitors = st.monitors.clone();
    let editable = !st.locked; // LOCK LIVE: Identify / Go live stay, editing does not
    egui::ScrollArea::vertical().id_salt("outputs_panel").show(ui, |ui| {
        ui.horizontal(|ui| {
            if ui.add_enabled(editable, egui::Button::new("+ Output")).clicked() {
                st.project.outputs.push(OutputConfig { name: format!("Output {}", st.project.outputs.len() + 1), ..OutputConfig::default() });
                act.outputs_changed = true;
            }
            if ui.button("Identify (5 s)").on_hover_text("Numbered test pattern on every output").clicked() {
                act.identify = true;
            }
        });
        ui.separator();
        let mut remove = None;
        let n = st.project.outputs.len();
        egui::ScrollArea::vertical().max_height(600.0).show(ui, |ui| {
            if !editable {
                ui.disable();
            }
            for (i, o) in st.project.outputs.iter_mut().enumerate() {
                let before = o.clone();
                ui.push_id(i, |ui| {
                    ui.horizontal(|ui| {
                        ui.strong(format!("{}.", i + 1));
                        ui.text_edit_singleline(&mut o.name);
                        let missing = o.monitor.as_ref().is_some_and(|m| !monitors.contains(m));
                        let label = match &o.monitor {
                            Some(m) if missing => format!("{m} (disconnected)"),
                            Some(m) => m.clone(),
                            None => "Window".into(),
                        };
                        egui::ComboBox::from_id_salt("monitor").selected_text(label).show_ui(ui, |ui| {
                            ui.selectable_value(&mut o.monitor, None, "Window");
                            for m in &monitors {
                                ui.selectable_value(&mut o.monitor, Some(m.clone()), m);
                            }
                        });
                        if n > 1 && ui.small_button("✖").clicked() {
                            remove = Some(i);
                        }
                    });
                    let other_gpu = o.monitor.as_ref().and_then(|m| st.display_gpus.iter().find(|(d, _)| d == m)).map(|(_, g)| g).filter(|g| !st.engine_gpu.is_empty() && **g != st.engine_gpu);
                    if let Some(g) = other_gpu {
                        ui.colored_label(
                            Color32::from_rgb(255, 170, 60),
                            format!("⚠ This screen is driven by {g}, the show renders on {}: every frame is copied between GPUs (slower). Use a port wired to {} if the laptop has one.", st.engine_gpu, st.engine_gpu),
                        );
                    }
                    ui.horizontal(|ui| {
                        ui.label("Source");
                        let text = match o.source {
                            OutputSource::Composition => "Composition".to_string(),
                            OutputSource::Layer(l) => format!("Layer {}", l + 1),
                        };
                        egui::ComboBox::from_id_salt("source").selected_text(text).show_ui(ui, |ui| {
                            ui.selectable_value(&mut o.source, OutputSource::Composition, "Composition");
                            for l in 0..layers {
                                ui.selectable_value(&mut o.source, OutputSource::Layer(l), format!("Layer {}", l + 1));
                            }
                        });
                        ui.checkbox(&mut o.test_pattern, "Test pattern");
                    });
                    ui.horizontal(|ui| {
                        ui.label("Slices");
                        if ui.small_button("Full").clicked() {
                            o.slices = vec![Slice::full()];
                        }
                        for c in [2, 3, 4] {
                            if ui.small_button(format!("{c} columns")).clicked() {
                                o.slices = Slice::columns(c);
                            }
                        }
                        if ui.small_button("+ Slice").clicked() {
                            o.slices.push(Slice { name: format!("Slice {}", o.slices.len() + 1), ..Slice::full() });
                        }
                    });
                    let mut drop = None;
                    for (k, s) in o.slices.iter_mut().enumerate() {
                        ui.push_id(k, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(format!("{}", k + 1));
                                ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(90.0));
                                if ui.small_button("✖").clicked() {
                                    drop = Some(k);
                                }
                            });
                            rect_editor(ui, "  from", &mut s.input);
                            rect_editor(ui, "  to  ", &mut s.output);
                        });
                    }
                    if let Some(k) = drop.filter(|_| o.slices.len() > 1) {
                        o.slices.remove(k);
                    }
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.small("source areas");
                            slice_map(ui, &o.slices, true);
                        });
                        ui.vertical(|ui| {
                            ui.small("on the screen");
                            slice_map(ui, &o.slices, false);
                        });
                        if let Some(Some(tex)) = previews.get(i) {
                            ui.vertical(|ui| {
                                ui.small("preview");
                                ui.image((tex.id(), vec2(160.0, 90.0)));
                            });
                        }
                    });
                });
                for s in o.slices.iter_mut() {
                    *s = s.clone().clamped();
                }
                if *o != before {
                    act.outputs_changed = true;
                }
                ui.separator();
            }
        });
        if let Some(i) = remove {
            st.project.outputs.remove(i);
            act.outputs_changed = true;
        }
    });
    if act.outputs_changed {
        st.dirty = true;
    }
}

/// The Performance panel (dock tab).
pub fn performance_body(ui: &mut egui::Ui, _st: &mut UiState, snap: &Snapshot) {
    egui::ScrollArea::vertical().id_salt("perf_panel").show(ui, |ui| {
        ui.label(format!("{} — {:.1} fps · p99 {:.1} ms · dropped {} · GPU {:.1} ms/frame", snap.adapter, snap.fps, snap.p99_ms, snap.dropped, snap.gpu_ms));
        ui.label(format!("GPU memory {:.0} MB · outputs {} · GPU resets {} · audio underruns {}", snap.gpu_memory_mb, snap.outputs, snap.device_resets, snap.audio_underruns));
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 90.0), egui::Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 2.0, Color32::from_gray(20));
        let max_ms = 50.0;
        let y = |ms: f32| rect.max.y - (ms.min(max_ms) / max_ms) * rect.height();
        for (ms, c) in [(16.7, Color32::from_rgb(60, 140, 60)), (33.3, Color32::from_rgb(160, 110, 40))] {
            p.line_segment([egui::pos2(rect.min.x, y(ms)), egui::pos2(rect.max.x, y(ms))], Stroke::new(1.0, c));
        }
        let n = snap.frame_times.len().max(2);
        let pts: Vec<egui::Pos2> = snap
            .frame_times
            .iter()
            .enumerate()
            .map(|(i, t)| egui::pos2(rect.min.x + rect.width() * i as f32 / (n - 1) as f32, y(*t)))
            .collect();
        p.add(egui::Shape::line(pts, Stroke::new(1.5, Color32::from_rgb(120, 200, 255))));
        ui.small("frame time (green 16.7 ms = 60 fps, orange 33.3 ms)");
        ui.separator();
        egui::Grid::new("perf_layers").striped(true).show(ui, |ui| {
            for (i, l) in snap.layers.iter().enumerate() {
                ui.label(format!("L{}", i + 1));
                ui.label(l.clip_name.as_deref().unwrap_or("—"));
                ui.label(l.kind.map(|k| format!("{k:?}")).unwrap_or_default());
                match &l.error {
                    Some(e) => ui.colored_label(Color32::LIGHT_RED, e),
                    None => ui.label(if l.loading { "loading" } else { "" }),
                };
                ui.end_row();
            }
        });
    });
}
