//! Under PROGRAM: one countdown row per layer on air. Under PREVIEW: play/pause, stop, time left.
use crate::shortcuts::AppAction;
use crate::ui::{Actions, UiState, layer_color};
use egui::{Color32, RichText, vec2};
use evj_engine::{Command, Snapshot};

pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0).round() as u64;
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}") } else { format!("{m}:{s:02}") }
}

/// `m:ss.mmm` (or `h:mm:ss.mmm`) — positions and time left on the monitors.
pub fn clock_ms(secs: f64) -> String {
    let ms = (secs.max(0.0) * 1000.0).round() as u64;
    let (s, frac) = (ms / 1000, ms % 1000);
    let (h, m, s) = (s / 3600, (s / 60) % 60, s % 60);
    if h > 0 { format!("{h}:{m:02}:{s:02}.{frac:03}") } else { format!("{m}:{s:02}.{frac:03}") }
}

/// Time left, with milliseconds.
pub fn countdown(secs: f64) -> String {
    if secs.is_finite() { format!("-{}", clock_ms(secs)) } else { "—".into() }
}

/// The last ten seconds before a clip ends: the countdown turns red and blinks (a loop never ends).
pub fn warn(secs: f64, looping: bool) -> bool {
    !looping && secs.is_finite() && secs < 10.0
}

/// A layer is on air when it shows a picture or plays sound (walk-in music has no picture).
pub fn on_air(l: &evj_engine::LayerState) -> bool {
    l.clip_name.is_some() && (l.has_frame || l.audio)
}

pub fn program_strip(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, act: &mut Actions) {
    let now = ui.input(|i| i.time);
    let key = st.shortcuts.combo(AppAction::SeqNext).map(|c| format!(" ({c})")).unwrap_or_default();
    let chain_color = Color32::from_rgb(80, 200, 255);
    let mut any = false;
    let mut next: Option<Option<usize>> = None; // Some(None) = the scene chain, Some(Some(i)) = layer run i
    if let Some(r) = &st.scene_run {
        ui.horizontal(|ui| {
            let end = if r.chain.looping { " ⟲" } else { " ■" };
            ui.label(RichText::new(format!("CHAIN {}/{} · Scene{end}", r.idx + 1, r.chain.cols.len())).small().strong().color(chain_color));
            if ui.small_button("⏭").on_hover_text(format!("Next scene now{key}")).clicked() {
                next = Some(None);
            }
        });
    }
    // Layer 1 (drawn on top) first.
    for (i, l) in snap.layers.iter().enumerate() {
        let Some(name) = l.clip_name.as_ref().filter(|_| on_air(l)) else { continue };
        if st.project.composition.layers.get(i).is_some_and(|p| p.bypass) {
            continue;
        }
        any = true;
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(4.0, 18.0), egui::Sense::hover());
            ui.painter().rect_filled(r, 1.0, layer_color(i));
            ui.label(RichText::new(format!("L{} {name}", i + 1)).small());
            // The layer chain whose latest step plays here.
            let run = st.layer_runs.iter().position(|r| r.chain.steps.get(r.step).is_some_and(|s| s.layer == i));
            if let Some(k) = run {
                let r = &st.layer_runs[k];
                let end = if r.chain.looping { " ⟲" } else { " ■" };
                ui.label(RichText::new(format!("CHAIN {}/{} · Layer{end}", r.step + 1, r.chain.steps.len())).small().color(chain_color));
                if ui.small_button("⏭").on_hover_text(format!("Next layer of the chain now{key}")).clicked() {
                    next = Some(Some(k));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                match l.remaining {
                    Some(rem) => {
                        let color = if warn(rem, l.looping) {
                            if (now * 2.0).fract() < 0.5 { Color32::from_rgb(255, 90, 90) } else { Color32::from_rgb(170, 40, 40) }
                        } else {
                            Color32::WHITE
                        };
                        ui.label(RichText::new(countdown(rem)).monospace().size(18.0).strong().color(color));
                        if l.looping {
                            ui.label(RichText::new("LOOP").small().weak());
                        }
                        if warn(rem, l.looping) {
                            ui.ctx().request_repaint();
                        }
                    }
                    None => {
                        ui.label(RichText::new("still").small().weak());
                    }
                }
                if l.duration > 0.0 {
                    let frac = (l.pos / l.duration).clamp(0.0, 1.0) as f32;
                    let w = ui.available_width().max(40.0);
                    ui.add(egui::ProgressBar::new(frac).desired_width(w).desired_height(6.0).fill(layer_color(i)));
                }
            });
        });
    }
    if !any {
        ui.label(RichText::new("Nothing on air").small().weak());
    }
    match next {
        Some(None) => crate::chain::next_scene(st, act),
        Some(Some(k)) => crate::chain::next_layer_run(st, k, act),
        None => {}
    }
}

pub fn preview_controls(ui: &mut egui::Ui, st: &UiState, snap: &Snapshot, act: &mut Actions) {
    let key = |a: AppAction| st.shortcuts.combo(a).map(|c| format!(" ({c})")).unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        let paused = snap.cue_state.as_ref().is_some_and(|c| c.paused);
        let has = snap.cue_state.is_some();
        let play = ui.add_enabled(has, egui::Button::new(if paused { "▶" } else { "⏸" }));
        if play.on_hover_text(format!("Play / pause preview{}", key(AppAction::PreviewPlayPause))).clicked() {
            act.commands.push(Command::CuePause(!paused));
        }
        let stop = ui.add_enabled(has, egui::Button::new("■"));
        if stop.on_hover_text(format!("Stop (back to start){}", key(AppAction::PreviewStop))).clicked() {
            act.commands.push(Command::CueRewind);
        }
        if let Some(c) = &snap.cue_state {
            ui.label(RichText::new(format!("{} / {}", clock_ms(c.pos), clock_ms(c.duration))).monospace().small());
            if let Some(r) = c.remaining {
                ui.label(RichText::new(countdown(r)).monospace().color(Color32::from_rgb(60, 200, 90)));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_formats() {
        assert_eq!(clock(45.4), "0:45");
        assert_eq!(clock(723.0), "12:03");
        assert_eq!(clock(3723.0), "1:02:03");
        assert_eq!(countdown(45.0), "-0:45.000");
        assert_eq!(countdown(3723.0456), "-1:02:03.046");
        assert_eq!(clock_ms(12.5), "0:12.500");
        assert_eq!(countdown(f64::INFINITY), "—");
    }

    #[test]
    fn audio_only_layers_count_as_on_air() {
        let mut l = evj_engine::LayerState { clip_name: Some("walk-in.mp3".into()), ..Default::default() };
        assert!(!on_air(&l), "nothing playing yet");
        l.audio = true;
        assert!(on_air(&l), "sound only, no picture");
        l.audio = false;
        l.has_frame = true;
        assert!(on_air(&l));
    }

    #[test]
    fn warning_under_ten_seconds() {
        assert!(warn(9.9, false));
        assert!(!warn(10.0, false));
        assert!(!warn(f64::INFINITY, false));
        assert!(!warn(3.0, true), "a loop (a 5 s image) does not end: no alarm");
    }
}
