//! Audio level meters: dBFS scale, colour zones, peak hold and the CLIP lamp.
use egui::{Color32, Rect, Sense, vec2};

pub const FLOOR_DB: f32 = -60.0;
const HOLD_SECS: f64 = 1.5;

pub fn db(v: f32) -> f32 {
    if v <= 1e-6 { -120.0 } else { 20.0 * v.log10() }
}

/// −60…0 dB → 0…1 along the meter.
pub fn level(db: f32) -> f32 {
    ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Green,
    Yellow,
    Red,
}

pub fn zone(db: f32) -> Zone {
    if db > -6.0 {
        Zone::Red
    } else if db > -18.0 {
        Zone::Yellow
    } else {
        Zone::Green
    }
}

fn color(z: Zone) -> Color32 {
    match z {
        Zone::Green => Color32::from_rgb(70, 200, 110),
        Zone::Yellow => Color32::from_rgb(235, 190, 50),
        Zone::Red => Color32::from_rgb(235, 60, 60),
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Hold {
    value: f32,
    at: f64,
}

impl Hold {
    /// Highest recent value; released 1.5 s after it was set.
    pub fn update(&mut self, v: f32, now: f64) -> f32 {
        if v >= self.value || now - self.at > HOLD_SECS {
            self.value = v;
            self.at = now;
        }
        self.value
    }
}

/// Fills `r` with green / yellow / red segments up to `v` and draws the hold line.
fn bar(p: &egui::Painter, r: Rect, v: f32, hold: f32, vertical: bool) {
    p.rect_filled(r, 1.0, Color32::from_gray(28));
    let lv = level(db(v));
    for (from, to, z) in [(FLOOR_DB, -18.0, Zone::Green), (-18.0, -6.0, Zone::Yellow), (-6.0, 0.0, Zone::Red)] {
        let (a, b) = (level(from), level(to).min(lv));
        if b <= a {
            continue;
        }
        let seg = if vertical {
            Rect::from_min_max(egui::pos2(r.min.x, r.max.y - r.height() * b), egui::pos2(r.max.x, r.max.y - r.height() * a))
        } else {
            Rect::from_min_max(egui::pos2(r.min.x + r.width() * a, r.min.y), egui::pos2(r.min.x + r.width() * b, r.max.y))
        };
        p.rect_filled(seg, 0.0, color(z));
    }
    let h = level(db(hold));
    if h > 0.0 {
        let stroke = egui::Stroke::new(1.5, Color32::WHITE);
        if vertical {
            let y = r.max.y - r.height() * h;
            p.line_segment([egui::pos2(r.min.x, y), egui::pos2(r.max.x, y)], stroke);
        } else {
            let x = r.min.x + r.width() * h;
            p.line_segment([egui::pos2(x, r.min.y), egui::pos2(x, r.max.y)], stroke);
        }
    }
}

/// Stereo vertical meter with the peak value (dBFS) under it.
pub fn vmeter(ui: &mut egui::Ui, peaks: [f32; 2], holds: &mut [Hold; 2], now: f64, height: f32) {
    ui.vertical(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(18.0, height), Sense::hover());
        let p = ui.painter();
        for c in 0..2 {
            let x0 = r.min.x + c as f32 * 9.0;
            let hold = holds[c].update(peaks[c], now);
            bar(p, Rect::from_min_size(egui::pos2(x0, r.min.y), vec2(8.0, r.height())), peaks[c], hold, true);
        }
        let peak_db = db(peaks[0].max(peaks[1]));
        let text = if peak_db <= FLOOR_DB { "-∞".to_string() } else { format!("{peak_db:.0}") };
        ui.label(egui::RichText::new(text).small().color(color(zone(peak_db))));
    });
}

/// Stereo horizontal mini meter (layer strips).
pub fn hmeter(ui: &mut egui::Ui, peaks: [f32; 2], holds: &mut [Hold; 2], now: f64, width: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(width, 9.0), Sense::hover());
    let p = ui.painter();
    for c in 0..2 {
        let hold = holds[c].update(peaks[c], now);
        bar(p, Rect::from_min_size(egui::pos2(r.min.x, r.min.y + c as f32 * 5.0), vec2(width, 4.0)), peaks[c], hold, false);
    }
}

/// Red while the output clipped since the last click; returns true when clicked (reset).
pub fn clip_lamp(ui: &mut egui::Ui, on: bool) -> bool {
    let fill = if on { Color32::from_rgb(235, 60, 60) } else { Color32::from_gray(45) };
    let text = egui::RichText::new("CLIP").small().strong().color(if on { Color32::WHITE } else { Color32::from_gray(110) });
    ui.add(egui::Button::new(text).fill(fill).small()).on_hover_text("Lights at 0 dBFS; click to reset").clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_and_zones() {
        assert_eq!(db(1.0), 0.0);
        assert!((db(0.5) + 6.02).abs() < 0.01);
        assert_eq!(db(0.0), -120.0, "silence floors instead of -inf");
        assert_eq!(level(0.0), 1.0);
        assert_eq!(level(-60.0), 0.0);
        assert_eq!(zone(-30.0), Zone::Green);
        assert_eq!(zone(-10.0), Zone::Yellow);
        assert_eq!(zone(-3.0), Zone::Red);
    }

    #[test]
    fn peak_hold_decays_after_1_5_s() {
        let mut h = Hold::default();
        assert_eq!(h.update(0.8, 0.0), 0.8);
        assert_eq!(h.update(0.1, 1.0), 0.8, "held");
        assert_eq!(h.update(0.1, 2.0), 0.1, "released");
    }
}
