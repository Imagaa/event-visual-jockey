//! Layer controls that look different and take typed values: opacity (% fill bar) and volume (dB fader).
use egui::{Color32, Sense, vec2};

pub const MIN_DB: f32 = -60.0;
pub const MAX_DB: f32 = 6.0;

pub fn gain_to_db(g: f32) -> f32 {
    if g <= 0.001 { MIN_DB } else { (20.0 * g.log10()).clamp(MIN_DB, MAX_DB) }
}

pub fn db_to_gain(db: f32) -> f32 {
    if db <= MIN_DB { 0.0 } else { 10f32.powf(db.min(MAX_DB) / 20.0) }
}

fn number(s: &str, suffix: &str) -> Option<f64> {
    let t = s.trim().to_ascii_lowercase();
    let t = t.trim_end_matches(suffix).trim();
    let v: f64 = t.parse().ok()?;
    v.is_finite().then_some(v)
}

pub fn parse_percent(s: &str) -> Option<f64> {
    number(s, "%").map(|v| v.clamp(0.0, 100.0))
}

pub fn parse_db(s: &str) -> Option<f64> {
    let t = s.trim().to_ascii_lowercase();
    if t.starts_with("-inf") || t == "-∞" {
        return Some(MIN_DB as f64);
    }
    number(&t, "db").map(|v| v.clamp(MIN_DB as f64, MAX_DB as f64))
}

/// Opacity: a fill bar in the layer colour (drag or click) + a typable percent box.
pub fn opacity_bar(ui: &mut egui::Ui, value: &mut f32, color: Color32) -> bool {
    let before = *value;
    ui.horizontal(|ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(96.0, 14.0), Sense::click_and_drag());
        if let Some(p) = resp.interact_pointer_pos() {
            *value = ((p.x - r.min.x) / r.width()).clamp(0.0, 1.0);
        }
        let painter = ui.painter();
        painter.rect_filled(r, 2.0, Color32::from_gray(35));
        painter.rect_filled(egui::Rect::from_min_size(r.min, vec2(r.width() * *value, r.height())), 2.0, color.gamma_multiply(0.8));
        resp.on_hover_text("Opacity");
        let mut pct = (*value as f64) * 100.0;
        let dv = egui::DragValue::new(&mut pct).range(0.0..=100.0).speed(0.5).max_decimals(0).suffix("%").custom_parser(parse_percent);
        if ui.add(dv).changed() {
            *value = (pct / 100.0) as f32;
        }
    });
    *value != before
}

/// Volume: a console-style fader in dB (−∞…+6, detent at 0 dB, double-click = 0 dB) + a typable dB box.
pub fn volume_fader(ui: &mut egui::Ui, gain: &mut f32) -> bool {
    let before = *gain;
    ui.horizontal(|ui| {
        let (r, resp) = ui.allocate_exact_size(vec2(96.0, 14.0), Sense::click_and_drag());
        let to_x = |db: f32| r.min.x + r.width() * (db - MIN_DB) / (MAX_DB - MIN_DB);
        if resp.double_clicked() {
            *gain = 1.0;
        } else if let Some(p) = resp.interact_pointer_pos() {
            let mut db = MIN_DB + (p.x - r.min.x) / r.width() * (MAX_DB - MIN_DB);
            if db.abs() < 1.0 {
                db = 0.0; // detent
            }
            *gain = db_to_gain(db);
        }
        let painter = ui.painter();
        let track = egui::Rect::from_center_size(r.center(), vec2(r.width(), 3.0));
        painter.rect_filled(track, 1.0, Color32::from_gray(60));
        for db in [-40.0, -20.0, -10.0, 0.0] {
            let x = to_x(db);
            let shade = if db == 0.0 { 150 } else { 80 };
            painter.line_segment([egui::pos2(x, r.min.y + 2.0), egui::pos2(x, r.max.y - 2.0)], egui::Stroke::new(1.0, Color32::from_gray(shade)));
        }
        let knob = egui::Rect::from_center_size(egui::pos2(to_x(gain_to_db(*gain)), r.center().y), vec2(8.0, 14.0));
        painter.rect_filled(knob, 2.0, Color32::from_gray(210));
        resp.on_hover_text("Volume (double-click = 0 dB)");
        let mut db = gain_to_db(*gain) as f64;
        let dv = egui::DragValue::new(&mut db)
            .range(MIN_DB as f64..=MAX_DB as f64)
            .speed(0.2)
            .max_decimals(1)
            .custom_formatter(|v, _| if v <= MIN_DB as f64 { "-∞ dB".into() } else { format!("{v:.1} dB") })
            .custom_parser(parse_db);
        if ui.add(dv).changed() {
            *gain = db_to_gain(db as f32);
        }
    });
    *gain != before
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gain_db_round_trip() {
        assert_eq!(gain_to_db(1.0), 0.0);
        assert!((db_to_gain(-6.0) - 0.501).abs() < 0.001);
        assert_eq!(db_to_gain(-60.0), 0.0, "fader bottom is silence");
        assert!((db_to_gain(6.0) - 1.995).abs() < 0.01);
    }

    #[test]
    fn parse_rejects_garbage_and_handles_inf() {
        assert_eq!(parse_percent("75%"), Some(75.0));
        assert_eq!(parse_percent(" 40 "), Some(40.0));
        assert_eq!(parse_percent("150"), Some(100.0), "clamped");
        assert_eq!(parse_percent("abc"), None);
        assert_eq!(parse_percent(""), None);
        assert_eq!(parse_db("-6 dB"), Some(-6.0));
        assert_eq!(parse_db("-inf"), Some(-60.0));
        assert_eq!(parse_db("+12"), Some(6.0), "clamped to +6");
        assert_eq!(parse_db("NaN"), None);
    }
}
