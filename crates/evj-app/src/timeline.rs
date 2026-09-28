//! Timeline of the selected clip: seek bar with start / end markers, waveform, attached audio.
use crate::thumbs::Thumbnailer;
use crate::ui::{Actions, Menu, Selection, UiState, layer_color};
use crate::waveform::Waveforms;
use egui::{Color32, Rect, RichText, Sense, Stroke, pos2, vec2};
use evj_core::model::{AttachedAudio, Clip, PlayMode};
use evj_engine::{Command, Snapshot};

/// Shortest start–end range (seconds).
pub const MIN_LEN: f64 = 0.05;

const PREVIEW: Color32 = Color32::from_rgb(60, 200, 90);
const PROGRAM: Color32 = Color32::from_rgb(235, 60, 60);

/// What a drag on the bar moves.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Drag {
    In,
    Out,
    Seek,
}

/// Sequential (H.264) sources re-open the file on every seek: while dragging, at most one
/// seek every 150 ms (and always on release). Random-access sources follow every move.
pub fn seek_due(random: bool, now: f64, last: f64, released: bool) -> bool {
    random || released || now - last >= 0.15
}

pub fn x_to_secs(x: f32, left: f32, width: f32, duration: f64) -> f64 {
    (((x - left) / width.max(1.0)) as f64).clamp(0.0, 1.0) * duration.max(0.0)
}

pub fn set_in(clip: &mut Clip, secs: f64, duration: f64) {
    if duration <= 0.0 {
        return;
    }
    let max = clip.out_point - MIN_LEN / duration;
    clip.in_point = (secs / duration).clamp(0.0, max.max(0.0));
}

pub fn set_out(clip: &mut Clip, secs: f64, duration: f64) {
    if duration <= 0.0 {
        return;
    }
    let min = clip.in_point + MIN_LEN / duration;
    clip.out_point = (secs / duration).clamp(min.min(1.0), 1.0);
}

/// I / O: the Preview playhead becomes the start / end of the selected clip.
pub fn mark(st: &mut UiState, snap: &Snapshot, start: bool, act: &mut Actions) {
    if !crate::lock::allowed(st.locked, crate::lock::Op::Content) {
        st.status = "Locked: unlock to change start / end".into();
        return;
    }
    let (Selection::Slot(l, c), Some(cue)) = (st.selected, snap.cue_state.as_ref()) else { return };
    let playing = st.is_playing(l, c);
    let Some(clip) = st.project.deck_mut().and_then(|d| d.clip_mut(l, c)) else { return };
    if start {
        set_in(clip, cue.pos, cue.duration);
    } else {
        set_out(clip, cue.pos, cue.duration);
    }
    let clip = clip.clone();
    edited(st, l, c, clip, playing, act);
}

/// A clip setting changed: the show is dirty, the Preview / a playing clip follow it.
fn edited(st: &mut UiState, layer: usize, col: usize, clip: Clip, playing: bool, act: &mut Actions) {
    if playing {
        let clip = crate::sequence::as_played(st, layer, col, clip);
        act.commands.push(Command::UpdateClip { layer, clip });
    }
    st.dirty = true;
}

/// The bottom panel: timeline of the selected slot (or the clips of a selected scene).
pub fn panel(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, waves: &mut Waveforms, thumbs: &mut Thumbnailer, act: &mut Actions) {
    match st.selected {
        Selection::Slot(l, c) => clip_timeline(ui, st, snap, waves, thumbs, l, c, act),
        Selection::Scene(c) => {
            ui.strong(st.project.deck().map(|d| d.scene_name(c)).unwrap_or_default());
            let clips = st.scene_clips(c);
            for (l, clip) in clips.iter().enumerate().rev() {
                let Some(clip) = clip else { continue };
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(vec2(4.0, 16.0), Sense::hover());
                    ui.painter().rect_filled(r, 1.0, layer_color(l));
                    if ui.link(format!("L{} {}", l + 1, clip.name)).on_hover_text("Open this clip's timeline").clicked() {
                        st.selected = Selection::Slot(l, c);
                    }
                });
            }
        }
        _ => {
            ui.label(RichText::new("Click a clip to see its timeline here.").weak());
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn clip_timeline(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, waves: &mut Waveforms, thumbs: &mut Thumbnailer, l: usize, c: usize, act: &mut Actions) {
    let Some(mut clip) = st.project.deck().and_then(|d| d.clip(l, c)).cloned() else {
        ui.label(RichText::new("Empty slot.").weak());
        return;
    };
    if evj_core::slides::SlideDeck::is_deck(&clip.path) {
        crate::present::timeline_controls(ui, st, snap, thumbs, l, c, act);
        return;
    }
    let before = clip.clone();
    let content = crate::lock::allowed(st.locked, crate::lock::Op::Content);
    let playing = st.is_playing(l, c);
    let info = thumbs.info(&clip.path).cloned();
    // Clip length: what the Preview reports (stills timed by their audio), else the file's.
    let cue = snap.cue_state.as_ref().filter(|q| q.name == clip.name);
    let duration = cue.map(|q| q.duration).filter(|d| *d > 0.0).or(info.as_ref().map(|i| i.duration)).unwrap_or(0.0);
    let file_len = info.as_ref().map_or(0.0, |i| i.duration);
    if !playing {
        st.seek_program = false;
    }
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(4.0, 18.0), Sense::hover());
        ui.painter().rect_filled(r, 1.0, layer_color(l));
        ui.strong(&clip.name);
        ui.label(RichText::new(crate::transport::clock(duration)).monospace());
        ui.separator();
        ui.add_enabled_ui(content, |ui| {
            let mut looping = clip.mode == PlayMode::Loop;
            if ui.toggle_value(&mut looping, "⟲ Loop").on_hover_text("Loop between start and end").changed() {
                clip.mode = if looping { PlayMode::Loop } else { PlayMode::Once };
            }
        });
        ui.separator();
        ui.label("Seek:");
        if ui.selectable_label(!st.seek_program, RichText::new("Preview").color(PREVIEW)).clicked() {
            st.seek_program = false;
        }
        let prog = ui.add_enabled(playing, egui::Button::selectable(st.seek_program, RichText::new("Program").color(PROGRAM)));
        if prog.on_hover_text("Seek what the audience sees (only while this clip is on Program)").clicked() {
            st.seek_program = true;
        }
    });
    let w = ui.available_width().max(100.0);
    let (bar, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::click_and_drag());
    let painter = ui.painter_at(bar);
    painter.rect_filled(bar, 3.0, Color32::from_gray(22));
    let x_of = |secs: f64| bar.left() + (secs / duration.max(1e-9)) as f32 * bar.width();
    let markers = file_len > 0.0 && (duration - file_len).abs() < 1e-3;
    let (a, b) = if markers { (clip.in_point * file_len, clip.out_point * file_len) } else { (0.0, duration) };
    if duration > 0.0 {
        let range = Rect::from_x_y_ranges(x_of(a)..=x_of(b), bar.y_range());
        painter.rect_filled(range, 3.0, Color32::from_gray(52));
        for (secs, color) in [(snap.cue_state.as_ref().filter(|q| q.name == clip.name).map(|q| q.pos), PREVIEW), (playing.then(|| snap.layers.get(l).map(|s| s.pos)).flatten(), PROGRAM)] {
            if let Some(t) = secs {
                let x = x_of(t);
                painter.line_segment([pos2(x, bar.top()), pos2(x, bar.bottom())], Stroke::new(2.0, color));
            }
        }
        if markers {
            for x in [x_of(a), x_of(b)] {
                painter.rect_filled(Rect::from_center_size(pos2(x, bar.center().y), vec2(4.0, bar.height())), 1.0, Color32::from_rgb(255, 210, 90));
            }
        }
    } else {
        painter.text(bar.center(), egui::Align2::CENTER_CENTER, "still image", egui::FontId::proportional(12.0), Color32::from_gray(140));
    }
    // Drag a marker (content edit) or seek (show control).
    if duration > 0.0 {
        if resp.drag_started() || resp.clicked() {
            let px = resp.interact_pointer_pos().map_or(bar.left(), |p| p.x);
            st.tl_drag = Some(if markers && content && (px - x_of(a)).abs() < 6.0 {
                Drag::In
            } else if markers && content && (px - x_of(b)).abs() < 6.0 {
                Drag::Out
            } else {
                Drag::Seek
            });
        }
        if let (Some(d), Some(p)) = (st.tl_drag, resp.interact_pointer_pos()) {
            let t = x_to_secs(p.x, bar.left(), bar.width(), duration);
            match d {
                Drag::In => set_in(&mut clip, t, file_len),
                Drag::Out => set_out(&mut clip, t, file_len),
                Drag::Seek => {
                    let released = resp.drag_stopped() || resp.clicked();
                    let random = info.as_ref().is_none_or(|i| i.random_access);
                    let now = ui.input(|i| i.time);
                    let moved = (t - st.tl_last_seek).abs() > 1.0 / 60.0;
                    if (moved || released) && seek_due(random, now, st.tl_last_seek_at, released) {
                        (st.tl_last_seek, st.tl_last_seek_at) = (t, now);
                        act.commands.push(Command::Seek { layer: st.seek_program.then_some(l), secs: t });
                    }
                }
            }
        }
        if resp.drag_stopped() || resp.clicked() {
            st.tl_drag = None;
        }
    }
    // Waveforms: the clip's own sound, then the attached audio.
    let own = clip.audio && info.as_ref().is_some_and(|i| i.has_audio);
    let lanes: Vec<(std::path::PathBuf, Color32)> = own
        .then(|| (clip.path.clone(), Color32::from_rgb(120, 230, 200)))
        .into_iter()
        .chain(clip.attached.as_ref().filter(|x| !x.path.as_os_str().is_empty()).map(|x| (x.path.clone(), Color32::from_rgb(120, 170, 255))))
        .collect();
    for (path, color) in lanes {
        let (lane, _) = ui.allocate_exact_size(vec2(w, 22.0), Sense::hover());
        ui.painter().rect_filled(lane, 2.0, Color32::from_gray(16));
        let wave = waves.get(&path);
        if let crate::waveform::Wave::None = wave {
            ui.painter().text(lane.center(), egui::Align2::CENTER_CENTER, "no sound track", egui::FontId::proportional(10.0), Color32::from_gray(110));
        } else if let crate::waveform::Wave::Ready(peaks) = wave {
            let n = peaks.len().max(1);
            let mut x = lane.left();
            while x < lane.right() {
                let i = (((x - lane.left()) / lane.width()) * n as f32) as usize;
                let h = peaks[i.min(n - 1)] * lane.height() * 0.5;
                ui.painter().line_segment([pos2(x, lane.center().y - h), pos2(x, lane.center().y + h)], Stroke::new(1.0, color));
                x += 1.0;
            }
        } else {
            ui.painter().text(lane.center(), egui::Align2::CENTER_CENTER, "reading sound…", egui::FontId::proportional(10.0), Color32::from_gray(120));
        }
    }
    ui.horizontal(|ui| {
        ui.add_enabled_ui(content && markers, |ui| {
            let cue_pos = snap.cue_state.as_ref().filter(|q| q.name == clip.name).map(|q| q.pos);
            let key = |x: crate::shortcuts::AppAction| st.shortcuts.combo(x).map(|k| format!(" ({k})")).unwrap_or_default();
            if ui.button(format!("[ Set start{}", key(crate::shortcuts::AppAction::MarkIn))).on_hover_text("At the Preview playhead").clicked() {
                if let Some(t) = cue_pos {
                    set_in(&mut clip, t, file_len);
                }
            }
            if ui.button(format!("Set end ]{}", key(crate::shortcuts::AppAction::MarkOut))).clicked() {
                if let Some(t) = cue_pos {
                    set_out(&mut clip, t, file_len);
                }
            }
            if ui.button("Reset").on_hover_text("Whole clip").clicked() {
                (clip.in_point, clip.out_point) = (0.0, 1.0);
            }
        });
        ui.separator();
        ui.add_enabled_ui(content, |ui| match clip.attached.clone() {
            None => {
                if ui.button("♪ Attach audio…").on_hover_text("Play an audio file with this picture").clicked() {
                    act.menu = Some(Menu::AttachAudio(l, c));
                }
            }
            Some(mut at) => {
                let name = at.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                ui.label(RichText::new(format!("♪ {name}")).color(Color32::from_rgb(120, 170, 255)));
                ui.selectable_value(&mut at.mix, false, "Replace").on_hover_text("Mute the clip's own sound");
                ui.selectable_value(&mut at.mix, true, "Mix").on_hover_text("Play both");
                crate::widgets::volume_fader(ui, &mut at.volume);
                let remove = ui.small_button("✖").on_hover_text("Remove the attached audio").clicked();
                clip.attached = if remove { None } else { Some(at) };
            }
        });
    });
    if clip != before {
        let audio_changed = clip.attached != before.attached;
        if let Some(slot) = st.project.deck_mut().and_then(|d| d.clip_mut(l, c)) {
            *slot = clip.clone();
        }
        if audio_changed && playing {
            st.status = "Attached audio: takes effect the next time the clip is triggered".into();
        }
        edited(st, l, c, clip, playing, act);
    }
}

/// The attached audio picked in the file dialog.
pub fn attach(st: &mut UiState, deck: usize, layer: usize, col: usize, path: std::path::PathBuf) {
    if let Some(clip) = st.project.decks.get_mut(deck).and_then(|d| d.clip_mut(layer, col)) {
        clip.attached = Some(AttachedAudio { path, ..Default::default() });
        st.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evj_core::model::Clip;

    #[test]
    fn bar_position_maps_to_seconds() {
        assert_eq!(x_to_secs(100.0, 100.0, 200.0, 10.0), 0.0);
        assert_eq!(x_to_secs(200.0, 100.0, 200.0, 10.0), 5.0);
        assert_eq!(x_to_secs(999.0, 100.0, 200.0, 10.0), 10.0, "clamped");
    }

    #[test]
    fn markers_keep_order_and_a_minimum_length() {
        let mut c = Clip::new("a.mov".into());
        set_in(&mut c, 4.0, 10.0);
        assert!((c.in_point - 0.4).abs() < 1e-9);
        set_out(&mut c, 2.0, 10.0);
        assert!(c.out_point > c.in_point, "out cannot pass in");
        assert!((c.out_point - c.in_point) * 10.0 >= MIN_LEN - 1e-9);
        set_in(&mut c, 20.0, 10.0);
        assert!(c.in_point < c.out_point);
    }

    #[test]
    fn sequential_seeks_are_throttled_while_dragging() {
        assert!(seek_due(true, 1.00, 0.99, false), "random access: every move");
        assert!(!seek_due(false, 1.05, 1.00, false), "H.264: not every frame");
        assert!(seek_due(false, 1.20, 1.00, false));
        assert!(seek_due(false, 1.01, 1.00, true), "always on release");
    }

    #[test]
    fn i_and_o_set_the_markers_from_the_preview() {
        let mut st = UiState::new(evj_core::model::Project::new_default());
        st.project.decks[0].slots[0][0] = Some(Clip::new("a.mov".into()));
        st.selected = Selection::Slot(0, 0);
        let cue = evj_engine::CueState { name: "a".into(), pos: 3.0, duration: 10.0, remaining: None, paused: true, start: 0.0, end: 10.0 };
        let snap = Snapshot { cue_state: Some(cue), ..Default::default() };
        let mut act = Actions::default();
        mark(&mut st, &snap, true, &mut act);
        assert!((st.project.decks[0].slots[0][0].as_ref().unwrap().in_point - 0.3).abs() < 1e-9);
        st.locked = true;
        mark(&mut st, &snap, false, &mut act);
        assert_eq!(st.project.decks[0].slots[0][0].as_ref().unwrap().out_point, 1.0, "locked: no marker edits");
    }
}
