//! Transition UI: the "Next" bar, the Transition Manager window, and preset pickers.
use crate::ui::{Actions, UiState};
use evj_core::effect::{Kind, ParamValue};
use evj_core::transition::{CUT, Easing, TransitionPreset, TransitionTime};
use evj_engine::fx::EffectInfo;
use evj_engine::{Command, Snapshot};

/// Favourite presets as quick buttons; the selected one is used by the next trigger.
pub fn next_bar(ui: &mut egui::Ui, st: &mut UiState) {
    ui.label("Next:");
    let favorites: Vec<String> = st.project.transitions.iter().filter(|p| p.favorite).map(|p| p.name.clone()).collect();
    for name in favorites {
        let on = st.next_transition.as_deref() == Some(name.as_str());
        if ui.selectable_label(on, &name).on_hover_text("Used by the next trigger (click again for the layer default)").clicked() {
            st.next_transition = if on { None } else { Some(name) };
        }
    }
    if ui.button("Transitions…").clicked() {
        crate::dock::toggle(&mut st.layout, crate::dock::Panel::Transitions);
    }
}

/// Combo for a layer default / clip override. `none_label` names what `None` means.
pub fn picker(ui: &mut egui::Ui, salt: &str, value: &mut Option<String>, presets: &[TransitionPreset], none_label: &str) -> bool {
    let mut changed = false;
    let text = value.clone().unwrap_or_else(|| none_label.to_string());
    egui::ComboBox::from_id_salt(salt).selected_text(text).show_ui(ui, |ui| {
        changed |= ui.selectable_value(value, None, none_label).changed();
        for p in presets {
            changed |= ui.selectable_value(value, Some(p.name.clone()), &p.name).changed();
        }
    });
    changed
}

fn params(ui: &mut egui::Ui, values: &mut Vec<ParamValue>, info: &EffectInfo) -> bool {
    let mut changed = false;
    for d in &info.meta.params {
        let i = match values.iter().position(|v| v.name == d.name) {
            Some(i) => i,
            None => {
                values.push(ParamValue { name: d.name.clone(), value: d.default as f64, lfo: None });
                values.len() - 1
            }
        };
        ui.horizontal(|ui| {
            ui.label(&d.name);
            changed |= ui.add(egui::Slider::new(&mut values[i].value, d.min as f64..=d.max as f64)).changed();
        });
    }
    changed
}

/// The engine renders the selected preset's animation while the Transitions tab is open.
pub fn sync_preview(st: &mut UiState, act: &mut Actions) {
    let open = crate::dock::is_open(&st.layout, crate::dock::Panel::Transitions);
    let wanted = if open { st.project.transitions.get(st.selected_preset).map(|p| p.shader.clone()).filter(|s| s != CUT) } else { None };
    if wanted != st.previewing {
        act.commands.push(Command::PreviewTransition(wanted.clone()));
        st.previewing = wanted;
    }
}

/// The Transitions panel (dock tab). `preview` is the engine's looping preview texture.
pub fn manager_body(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, _act: &mut Actions, preview: Option<(egui::TextureId, [f32; 2])>) {
    let shaders: Vec<&EffectInfo> = snap.effects.iter().filter(|e| e.meta.kind == Kind::Transition).collect();
    egui::ScrollArea::vertical().id_salt("transitions_panel").show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(200.0);
                egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                    for (i, p) in st.project.transitions.iter().enumerate() {
                        let label = format!("{}{}", if p.favorite { "★ " } else { "" }, p.name);
                        if ui.selectable_label(st.selected_preset == i, label).clicked() {
                            st.selected_preset = i;
                        }
                    }
                });
                if ui.button("+ New preset").clicked() {
                    let mut p = TransitionPreset::crossfade();
                    p.name = format!("Preset {}", st.project.transitions.len() + 1);
                    p.favorite = false;
                    st.project.transitions.push(p);
                    st.selected_preset = st.project.transitions.len() - 1;
                    st.dirty = true;
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                if let Some((id, size)) = preview.filter(|_| st.previewing.is_some()) {
                    ui.image((id, egui::vec2(size[0], size[1])));
                } else {
                    ui.label("Cut: no animation");
                }
                let n = st.project.transitions.len();
                let Some(p) = st.project.transitions.get_mut(st.selected_preset) else { return };
                let before = p.clone();
                egui::Grid::new("preset").num_columns(2).show(ui, |ui| {
                    ui.label("Name");
                    ui.text_edit_singleline(&mut p.name);
                    ui.end_row();
                    ui.label("Shader");
                    egui::ComboBox::from_id_salt("shader").selected_text(&p.shader).show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.shader, CUT.to_string(), CUT);
                        for s in &shaders {
                            ui.selectable_value(&mut p.shader, s.meta.name.clone(), &s.meta.name);
                        }
                    });
                    ui.end_row();
                    ui.label("Duration");
                    ui.horizontal(|ui| {
                        let (mut v, mut beats) = match p.time {
                            TransitionTime::Seconds(s) => (s, false),
                            TransitionTime::Beats(b) => (b, true),
                        };
                        ui.add(egui::DragValue::new(&mut v).range(0.0..=60.0).speed(0.05).max_decimals(2));
                        ui.selectable_value(&mut beats, false, "sec");
                        ui.selectable_value(&mut beats, true, "beats");
                        p.time = if beats { TransitionTime::Beats(v) } else { TransitionTime::Seconds(v) };
                    });
                    ui.end_row();
                    ui.label("Easing");
                    egui::ComboBox::from_id_salt("easing").selected_text(format!("{:?}", p.easing)).show_ui(ui, |ui| {
                        for e in [Easing::Linear, Easing::In, Easing::Out, Easing::InOut] {
                            ui.selectable_value(&mut p.easing, e, format!("{e:?}"));
                        }
                    });
                    ui.end_row();
                    ui.label("Favorite");
                    ui.checkbox(&mut p.favorite, "show in the Next bar");
                    ui.end_row();
                });
                if let Some(info) = shaders.iter().find(|s| s.meta.name == p.shader) {
                    params(ui, &mut p.params, info);
                }
                if *p != before {
                    st.dirty = true;
                }
                if n > 1 && p.name != CUT && ui.button("Delete preset").clicked() {
                    st.project.transitions.remove(st.selected_preset);
                    st.selected_preset = st.selected_preset.saturating_sub(1);
                    st.dirty = true;
                }
            });
        });
        crate::fxui::errors(ui, &snap.effects);
    });
}
