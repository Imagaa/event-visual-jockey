//! Questions that need an answer: centred over the dimmed show, a title, a short message and
//! clear buttons. The main button is filled (red when it destroys something); Enter picks it,
//! Esc or a click outside picks Cancel.
use egui::{Color32, RichText, vec2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    /// The expected answer (filled blue).
    Primary,
    /// Removes something (filled red).
    Danger,
    Plain,
}

const WIDTH: f32 = 420.0;

/// Shows the question; the index of the button chosen this frame. Buttons are given left to
/// right; `cancel` is what Esc / a click outside means (None = an answer is required).
pub fn ask(ctx: &egui::Context, id: &str, title: &str, cancel: Option<usize>, buttons: &[(&str, Tone)], body: impl FnOnce(&mut egui::Ui)) -> Option<usize> {
    let mut chosen = None;
    let frame = egui::Frame::window(&ctx.global_style()).inner_margin(20.0).corner_radius(10.0);
    let resp = egui::Modal::new(egui::Id::new(id)).backdrop_color(Color32::from_black_alpha(170)).frame(frame).show(ctx, |ui| {
        ui.set_width(WIDTH);
        ui.label(RichText::new(title).size(19.0).strong().color(ui.visuals().strong_text_color()));
        ui.add_space(10.0);
        body(ui);
        ui.add_space(18.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for (i, (label, tone)) in buttons.iter().enumerate().rev() {
                let (fill, text) = match tone {
                    Tone::Primary => (Color32::from_rgb(40, 110, 200), Color32::WHITE),
                    Tone::Danger => (Color32::from_rgb(190, 45, 45), Color32::WHITE),
                    Tone::Plain => (ui.visuals().widgets.inactive.weak_bg_fill, ui.visuals().text_color()),
                };
                let b = egui::Button::new(RichText::new(*label).size(14.0).color(text).strong()).fill(fill).min_size(vec2(96.0, 32.0)).corner_radius(6.0);
                if ui.add(b).clicked() {
                    chosen = Some(i);
                }
            }
        });
    });
    // Enter = the main answer (a text field in the body takes Enter itself and loses focus).
    let main = buttons.iter().position(|(_, t)| *t != Tone::Plain);
    if chosen.is_none() && main.is_some() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        chosen = main;
    }
    if chosen.is_none() && cancel.is_some() && resp.should_close() {
        chosen = cancel;
    }
    chosen
}

/// A message line in a question.
pub fn text(ui: &mut egui::Ui, s: impl Into<String>) {
    ui.label(RichText::new(s.into()).size(14.0));
}

/// A quieter second line (consequences, hints).
pub fn note(ui: &mut egui::Ui, s: impl Into<String>) {
    ui.label(RichText::new(s.into()).size(12.5).weak());
}
