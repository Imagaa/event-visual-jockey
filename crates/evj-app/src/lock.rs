//! LOCK LIVE: during a show only show control works; anything that changes the show's content is blocked.
use crate::ui::UiState;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    /// Cue, take, trigger, clear, opacity, volume, BPM, transitions, blackout, panic, slides, pointer.
    Show,
    /// Add / remove / move media, drag-drop, layers / columns / decks, effects, outputs, New / Open / Import.
    Content,
}

pub fn allowed(locked: bool, op: Op) -> bool {
    !locked || op == Op::Show
}

#[derive(Default)]
pub struct Unlock {
    held_since: Option<f64>,
}

impl Unlock {
    /// Feed the button state each frame: (progress 0..1, unlock now).
    pub fn hold(&mut self, pressed: bool, now: f64) -> (f32, bool) {
        if !pressed {
            self.held_since = None;
            return (0.0, false);
        }
        let t0 = *self.held_since.get_or_insert(now);
        let p = (now - t0).min(1.0) as f32;
        if p >= 1.0 {
            self.held_since = None;
            return (1.0, true);
        }
        (p, false)
    }
}

/// 🔒 in the top bar: a click locks; unlocking needs the button held for 1 s (a bar fills).
pub fn toggle_button(ui: &mut egui::Ui, st: &mut UiState, now: f64) {
    if !st.locked {
        if ui.button("🔓 Lock").on_hover_text("LOCK LIVE: only show control works (hold to unlock)").clicked() {
            st.locked = true;
            st.status = "LOCKED — show control only. Hold the lock button 1 s to unlock.".into();
        }
        return;
    }
    let text = egui::RichText::new("🔒 LOCKED").strong().color(egui::Color32::from_rgb(255, 170, 60));
    let r = ui.add(egui::Button::new(text).sense(egui::Sense::click_and_drag())).on_hover_text("Hold 1 s to unlock");
    let (p, done) = st.unlock.hold(r.is_pointer_button_down_on(), now);
    if p > 0.0 {
        let y = r.rect.max.y - 1.0;
        let stroke = egui::Stroke::new(3.0, egui::Color32::from_rgb(255, 170, 60));
        ui.painter().line_segment([egui::pos2(r.rect.min.x, y), egui::pos2(r.rect.min.x + r.rect.width() * p, y)], stroke);
        ui.ctx().request_repaint();
    }
    if done {
        st.locked = false;
        st.status = "Unlocked".into();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_blocks_content_edits_only() {
        assert!(allowed(false, Op::Content));
        assert!(allowed(true, Op::Show));
        assert!(!allowed(true, Op::Content));
    }

    #[test]
    fn unlock_needs_one_second_of_holding() {
        let mut u = Unlock::default();
        assert_eq!(u.hold(true, 0.0), (0.0, false));
        let (p, done) = u.hold(true, 0.5);
        assert!((p - 0.5).abs() < 1e-6 && !done);
        assert_eq!(u.hold(false, 0.7), (0.0, false), "released early: start over");
        u.hold(true, 1.0);
        assert!(u.hold(true, 2.01).1);
    }
}
