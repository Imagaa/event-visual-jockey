//! Sound outputs of this laptop (Program, Preview), kept in settings.json next to the shortcuts.
use evj_audio::Route;
use evj_engine::{Bus, Command};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AudioSettings {
    pub program: Route,
    /// None = Preview sound off.
    pub preview: Option<Route>,
    pub preview_volume: f32,
}

impl Default for AudioSettings {
    fn default() -> Self {
        AudioSettings { program: Route::default(), preview: None, preview_volume: 1.0 }
    }
}

pub fn load(file: &Path) -> AudioSettings {
    let v: Option<serde_json::Value> = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok());
    v.and_then(|v| serde_json::from_value(v.get("audio")?.clone()).ok()).unwrap_or_default()
}

pub fn save(file: &Path, s: &AudioSettings) {
    if let Ok(v) = serde_json::to_value(s) {
        crate::shortcuts::write_section(file, "audio", v);
    }
}

/// Preview would play on the Program output: the same device (Windows default resolved by
/// name) and the same channel pair.
pub fn clash(program: &Route, preview: &Route, default_device: Option<&str>) -> bool {
    let name = |r: &Route| r.device.clone().or_else(|| default_device.map(str::to_string));
    name(program) == name(preview) && program.first_channel == preview.first_channel
}

/// What the engine needs at start: both routes and the Preview level.
pub fn commands(s: &AudioSettings) -> Vec<Command> {
    vec![
        Command::SetAudioRoute { bus: Bus::Program, route: Some(s.program.clone()) },
        Command::SetAudioRoute { bus: Bus::Preview, route: s.preview.clone() },
        Command::SetPreviewVolume(s.preview_volume),
    ]
}

/// "1-2", "3-4" … for a first channel (0-based).
pub fn pair_label(first: u16) -> String {
    format!("{}-{}", first + 1, first + 2)
}

/// The Audio outputs rows (Composition panel).
pub fn panel(ui: &mut egui::Ui, st: &mut crate::ui::UiState, snap: &evj_engine::Snapshot, act: &mut crate::ui::Actions) {
    use egui::{Color32, RichText};
    ui.strong("Audio outputs");
    // Asking Windows for its devices costs milliseconds: once, and again when a list opens.
    if st.audio_devices.is_empty() {
        st.audio_devices = evj_audio::AudioEngine::devices();
        st.default_audio_device = evj_audio::AudioEngine::default_device();
    }
    let devices = st.audio_devices.clone();
    let default = st.default_audio_device.clone();
    let channels = |d: &Option<String>| {
        let name = d.clone().or_else(|| default.clone());
        devices.iter().find(|(n, _)| Some(n) == name.as_ref()).map_or(2, |(_, c)| *c)
    };
    let mut program = st.audio.program.clone();
    let mut preview = st.audio.preview.clone();
    egui::Grid::new("audio_outputs").num_columns(3).show(ui, |ui| {
        ui.label(RichText::new("Program").color(Color32::from_rgb(235, 60, 60)));
        let shown = program.device.clone().unwrap_or_else(|| "Windows default".into());
        let r = egui::ComboBox::from_id_salt("prog_dev").selected_text(shown).width(220.0).show_ui(ui, |ui| {
            ui.selectable_value(&mut program.device, None, "Windows default");
            for (n, _) in &devices {
                ui.selectable_value(&mut program.device, Some(n.clone()), n);
            }
        });
        if r.response.clicked() {
            st.audio_devices = evj_audio::AudioEngine::devices();
            st.default_audio_device = evj_audio::AudioEngine::default_device();
        }
        pair_combo(ui, "prog_ch", &mut program.first_channel, channels(&program.device));
        ui.end_row();
        ui.label(RichText::new("Preview 🎧").color(Color32::from_rgb(60, 200, 90)));
        let shown = match &preview {
            None => "Off".to_string(),
            Some(r) => r.device.clone().unwrap_or_else(|| "Windows default".into()),
        };
        egui::ComboBox::from_id_salt("prev_dev").selected_text(shown).width(220.0).show_ui(ui, |ui| {
            if ui.selectable_label(preview.is_none(), "Off").clicked() {
                preview = None;
            }
            if ui.selectable_label(preview.as_ref().is_some_and(|r| r.device.is_none()), "Windows default").clicked() {
                preview = Some(Route::default());
            }
            for (n, _) in &devices {
                let on = preview.as_ref().is_some_and(|r| r.device.as_ref() == Some(n));
                if ui.selectable_label(on, n).clicked() {
                    preview = Some(Route { device: Some(n.clone()), first_channel: 0 });
                }
            }
        });
        if let Some(r) = preview.as_mut() {
            let ch = channels(&r.device);
            pair_combo(ui, "prev_ch", &mut r.first_channel, ch);
        }
        ui.end_row();
    });
    let refused = preview.as_ref().is_some_and(|p| clash(&program, p, default.as_deref()));
    if refused {
        st.audio_refused = true; // shown until the next choice that is fine
    }
    if st.audio_refused {
        ui.label(RichText::new("Preview would play on the sound system — pick other channels or another device").color(Color32::from_rgb(235, 80, 80)));
    }
    if let Some(e) = &snap.preview_audio_error {
        ui.label(RichText::new(format!("Preview audio off: {e}")).color(Color32::from_rgb(235, 80, 80)));
    }
    if program != st.audio.program {
        st.audio.program = program;
        act.commands.push(Command::SetAudioRoute { bus: Bus::Program, route: Some(st.audio.program.clone()) });
        st.audio_dirty = true;
    }
    if preview != st.audio.preview && !refused {
        st.audio_refused = false;
        st.audio.preview = preview;
        act.commands.push(Command::SetAudioRoute { bus: Bus::Preview, route: st.audio.preview.clone() });
        st.audio_dirty = true;
    }
}

fn pair_combo(ui: &mut egui::Ui, id: &str, first: &mut u16, channels: u16) {
    let pairs = evj_audio::channel_pairs(channels);
    if !pairs.contains(first) {
        *first = 0;
    }
    egui::ComboBox::from_id_salt(id).selected_text(pair_label(*first)).width(60.0).show_ui(ui, |ui| {
        for p in pairs {
            ui.selectable_value(first, p, pair_label(p));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(d: Option<&str>, c: u16) -> Route {
        Route { device: d.map(Into::into), first_channel: c }
    }

    #[test]
    fn the_same_device_and_pair_clashes() {
        assert!(clash(&r(Some("Speakers"), 0), &r(Some("Speakers"), 0), None));
        assert!(!clash(&r(Some("Speakers"), 0), &r(Some("Speakers"), 2), None), "3-4 on the same interface is fine");
        assert!(!clash(&r(Some("Speakers"), 0), &r(Some("Headphones"), 0), None));
        assert!(clash(&r(None, 0), &r(Some("Speakers"), 0), Some("Speakers")), "Windows default resolved by name");
    }

    #[test]
    fn settings_round_trip_and_keep_the_shortcuts() {
        let f = std::env::temp_dir().join(format!("evj-audioset-{}.json", std::process::id()));
        std::fs::write(&f, r#"{"shortcuts":{"Save":"Ctrl+S"}}"#).unwrap();
        assert_eq!(load(&f), AudioSettings::default(), "old file: defaults");
        let s = AudioSettings { program: r(Some("HDMI"), 0), preview: Some(r(Some("Headphones"), 0)), preview_volume: 0.5 };
        save(&f, &s);
        assert_eq!(load(&f), s);
        assert!(std::fs::read_to_string(&f).unwrap().contains("Ctrl+S"), "shortcuts kept");
        crate::shortcuts::Shortcuts::defaults().save(&f);
        assert_eq!(load(&f), s, "saving shortcuts keeps the audio settings");
        let _ = std::fs::remove_file(f);
    }

    #[test]
    fn startup_sends_both_routes_and_the_level() {
        let s = AudioSettings { preview: Some(r(Some("Headphones"), 0)), preview_volume: 0.5, ..Default::default() };
        let c = commands(&s);
        assert!(c.iter().any(|c| matches!(c, Command::SetAudioRoute { bus: Bus::Program, route: Some(_) })));
        assert!(c.iter().any(|c| matches!(c, Command::SetAudioRoute { bus: Bus::Preview, route: Some(_) })));
        assert!(c.iter().any(|c| matches!(c, Command::SetPreviewVolume(v) if *v == 0.5)));
    }

    #[test]
    fn channel_labels() {
        assert_eq!(pair_label(0), "1-2");
        assert_eq!(pair_label(2), "3-4");
    }
}
