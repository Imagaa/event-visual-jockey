//! App-wide keyboard shortcuts (EVJ focused), editable, stored in %APPDATA%\EVJ\settings.json.
//! The per-project keymap (slots / columns) is separate: `evj_core::keymap`.
use crate::ui::UiState;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum AppAction {
    New,
    Open,
    Save,
    SaveAs,
    AddColumn,
    AddLayer,
    PreviewPlayPause,
    PreviewStop,
    Take,
    ProgramStop,
    Blackout,
    Panic,
    LockToggle,
    TapTempo,
    ShowShortcuts,
    ShowPerf,
    ShowOutputs,
    MarkIn,
    MarkOut,
    SeqNext,
}

impl AppAction {
    pub const ALL: [AppAction; 20] = [
        AppAction::New,
        AppAction::Open,
        AppAction::Save,
        AppAction::SaveAs,
        AppAction::AddColumn,
        AppAction::AddLayer,
        AppAction::PreviewPlayPause,
        AppAction::PreviewStop,
        AppAction::Take,
        AppAction::ProgramStop,
        AppAction::Blackout,
        AppAction::Panic,
        AppAction::LockToggle,
        AppAction::TapTempo,
        AppAction::ShowShortcuts,
        AppAction::ShowPerf,
        AppAction::ShowOutputs,
        AppAction::MarkIn,
        AppAction::MarkOut,
        AppAction::SeqNext,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AppAction::New => "New show",
            AppAction::Open => "Open show",
            AppAction::Save => "Save",
            AppAction::SaveAs => "Save as",
            AppAction::AddColumn => "Add column",
            AppAction::AddLayer => "Add layer",
            AppAction::PreviewPlayPause => "Preview: play / pause",
            AppAction::PreviewStop => "Preview: stop (rewind)",
            AppAction::Take => "TAKE (Preview › Program)",
            AppAction::ProgramStop => "Program: stop (clear all layers)",
            AppAction::Blackout => "Blackout",
            AppAction::Panic => "PANIC",
            AppAction::LockToggle => "Lock live",
            AppAction::TapTempo => "Tap tempo",
            AppAction::ShowShortcuts => "Shortcuts window",
            AppAction::ShowPerf => "Performance window",
            AppAction::ShowOutputs => "Output Manager",
            AppAction::MarkIn => "Timeline: set start at the Preview playhead",
            AppAction::MarkOut => "Timeline: set end at the Preview playhead",
            AppAction::SeqNext => "Chain: next step / scene now",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct KeyCombo {
    pub key: egui::Key,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

/// egui key names are case-sensitive ("Enter", "F12"); accept "enter" / "f12" too.
fn key_from(part: &str) -> Option<egui::Key> {
    egui::Key::from_name(part).or_else(|| {
        let mut c = part.chars();
        let first = c.next()?;
        egui::Key::from_name(&(first.to_uppercase().collect::<String>() + &c.as_str().to_ascii_lowercase()))
    })
}

impl KeyCombo {
    pub fn parse(s: &str) -> Option<KeyCombo> {
        let mut c = KeyCombo { key: egui::Key::A, ctrl: false, shift: false, alt: false };
        let mut key = None;
        for part in s.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => c.ctrl = true,
                "shift" => c.shift = true,
                "alt" => c.alt = true,
                "" => return None,
                _ if key.is_none() => key = Some(key_from(part)?),
                _ => return None,
            }
        }
        c.key = key?;
        Some(c)
    }

    fn matches(&self, key: egui::Key, m: egui::Modifiers) -> bool {
        self.key == key && self.ctrl == m.ctrl && self.shift == m.shift && self.alt == m.alt
    }
}

impl std::fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.ctrl {
            write!(f, "Ctrl+")?;
        }
        if self.shift {
            write!(f, "Shift+")?;
        }
        if self.alt {
            write!(f, "Alt+")?;
        }
        write!(f, "{}", self.key.name())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shortcuts {
    pub map: Vec<(AppAction, KeyCombo)>,
}

/// On disk: { "shortcuts": { "Save": "Ctrl+S", ... } } — unknown or bad entries are skipped.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct SettingsFile {
    shortcuts: std::collections::BTreeMap<String, String>,
}

impl Shortcuts {
    pub fn defaults() -> Shortcuts {
        use AppAction::*;
        let d = [
            (New, "Ctrl+N"),
            (Open, "Ctrl+O"),
            (Save, "Ctrl+S"),
            (SaveAs, "Ctrl+Shift+S"),
            (AddColumn, "Ctrl+C"),
            (AddLayer, "Ctrl+L"),
            (PreviewPlayPause, "Space"),
            (PreviewStop, "Shift+Space"),
            (Take, "Enter"),
            (ProgramStop, "Shift+Enter"),
            (Blackout, "B"),
            (Panic, "F12"),
            (LockToggle, "Ctrl+Shift+L"),
            (TapTempo, "T"),
            (ShowShortcuts, "F1"),
            (ShowPerf, "F2"),
            (ShowOutputs, "F3"),
            (MarkIn, "I"),
            (MarkOut, "O"),
            (SeqNext, "N"),
        ];
        Shortcuts { map: d.iter().filter_map(|(a, s)| Some((*a, KeyCombo::parse(s)?))).collect() }
    }

    pub fn combo(&self, a: AppAction) -> Option<KeyCombo> {
        self.map.iter().find(|(x, _)| *x == a).map(|(_, c)| *c)
    }

    pub fn set(&mut self, a: AppAction, c: Option<KeyCombo>) {
        self.map.retain(|(x, _)| *x != a);
        if let Some(c) = c {
            self.map.push((a, c));
        }
    }

    /// The action bound to an unmodified `key`, if any (the project keymap must not reuse it).
    pub fn plain_key(&self, key: egui::Key) -> Option<AppAction> {
        self.map.iter().find(|(_, c)| c.key == key && !c.ctrl && !c.shift && !c.alt).map(|(a, _)| *a)
    }

    pub fn conflicts(&self) -> Vec<(AppAction, AppAction)> {
        let mut v = Vec::new();
        for (i, (a, ca)) in self.map.iter().enumerate() {
            for (b, cb) in &self.map[i + 1..] {
                if ca == cb {
                    v.push((*a, *b));
                }
            }
        }
        v
    }

    pub fn load(file: &Path) -> Shortcuts {
        let mut sc = Shortcuts::defaults();
        let parsed: Option<SettingsFile> = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok());
        let Some(f) = parsed else {
            if file.exists() {
                evj_core::log::warn("settings", &format!("{} is not valid; using default shortcuts", file.display()));
            }
            return sc;
        };
        for a in AppAction::ALL {
            if let Some(s) = f.shortcuts.get(&format!("{a:?}")) {
                let keep = sc.combo(a);
                sc.set(a, if s.is_empty() { None } else { KeyCombo::parse(s).or(keep) });
            }
        }
        sc
    }

    pub fn save(&self, file: &Path) {
        let shortcuts: std::collections::BTreeMap<String, String> = AppAction::ALL.iter().map(|a| (format!("{a:?}"), self.combo(*a).map(|c| c.to_string()).unwrap_or_default())).collect();
        if let Ok(v) = serde_json::to_value(shortcuts) {
            write_section(file, "shortcuts", v);
        }
    }
}

/// Replaces one section of settings.json, keeping the others (shortcuts, audio …).
pub fn write_section(file: &Path, key: &str, value: serde_json::Value) {
    let old: Option<serde_json::Value> = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok());
    let mut root = old.filter(serde_json::Value::is_object).unwrap_or_else(|| serde_json::json!({}));
    root[key] = value;
    if let Ok(json) = serde_json::to_string_pretty(&root) {
        let _ = evj_core::io::write_atomic(file, json.as_bytes());
    }
}

pub fn settings_file() -> Option<PathBuf> {
    Some(evj_core::io::app_dir()?.join("settings.json"))
}

fn is_fkey(k: egui::Key) -> bool {
    k.name().strip_prefix('F').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// App actions for this frame's key presses. While a text field has focus only Ctrl / Alt combos count.
pub fn resolve(sc: &Shortcuts, events: &[egui::Event], typing: bool) -> Vec<AppAction> {
    let mut out = Vec::new();
    for e in events {
        // egui-winit turns Ctrl+C / Ctrl+X into Copy / Cut events (no Key event).
        let (key, modifiers) = match e {
            egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => (*key, *modifiers),
            egui::Event::Copy if !typing => (egui::Key::C, egui::Modifiers::CTRL),
            egui::Event::Cut if !typing => (egui::Key::X, egui::Modifiers::CTRL),
            _ => continue,
        };
        // While typing only Ctrl / Alt combos and function keys (PANIC is F12) count.
        if typing && !modifiers.ctrl && !modifiers.alt && !is_fkey(key) {
            continue;
        }
        if let Some((a, _)) = sc.map.iter().find(|(_, c)| c.matches(key, modifiers)) {
            out.push(*a);
        }
    }
    out
}

/// Editor: click a shortcut, press the new combination (Esc = remove, Backspace = default).
pub fn body(ui: &mut egui::Ui, st: &mut UiState) {
    egui::ScrollArea::vertical().id_salt("shortcuts_panel").show(ui, |ui| {
        ui.label(egui::RichText::new("Click a shortcut, then press the new keys. Esc removes it, Backspace restores the default.").small().weak());
        if let Some(armed) = st.shortcut_armed {
            let pressed = ui.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } => Some((*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some((key, m)) = pressed {
                let new = match key {
                    egui::Key::Escape => None,
                    egui::Key::Backspace => Shortcuts::defaults().combo(armed),
                    _ => Some(KeyCombo { key, ctrl: m.ctrl, shift: m.shift, alt: m.alt }),
                };
                st.shortcuts.set(armed, new);
                st.shortcut_armed = None;
                if let Some(f) = settings_file() {
                    st.shortcuts.save(&f);
                }
            }
        }
        let conflicts = st.shortcuts.conflicts();
        egui::Grid::new("shortcuts").num_columns(2).striped(true).show(ui, |ui| {
            for a in AppAction::ALL {
                ui.label(a.label());
                let text = match (st.shortcut_armed == Some(a), st.shortcuts.combo(a)) {
                    (true, _) => "press keys…".to_string(),
                    (false, Some(c)) => c.to_string(),
                    (false, None) => "—".to_string(),
                };
                let clash = conflicts.iter().any(|(x, y)| *x == a || *y == a);
                let rich = egui::RichText::new(text).monospace();
                let rich = if clash { rich.color(egui::Color32::from_rgb(255, 150, 40)) } else { rich };
                if ui.button(rich).on_hover_text(if clash { "Used by another action" } else { "Click to change" }).clicked() {
                    st.shortcut_armed = Some(a);
                }
                ui.end_row();
            }
        });
        if ui.button("Restore all defaults").clicked() {
            st.shortcuts = Shortcuts::defaults();
            if let Some(f) = settings_file() {
                st.shortcuts.save(&f);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(k: egui::Key, ctrl: bool, shift: bool) -> egui::Event {
        let mut m = egui::Modifiers::NONE;
        m.ctrl = ctrl;
        m.command = ctrl;
        m.shift = shift;
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: m }
    }

    #[test]
    fn defaults_are_unique() {
        assert!(Shortcuts::defaults().conflicts().is_empty());
        assert_eq!(Shortcuts::defaults().combo(AppAction::Save), KeyCombo::parse("Ctrl+S"));
        assert_eq!(Shortcuts::defaults().map.len(), AppAction::ALL.len(), "every default parses");
    }

    #[test]
    fn combos_parse_and_print() {
        let c = KeyCombo::parse("ctrl+shift+l").unwrap();
        assert_eq!(c.to_string(), "Ctrl+Shift+L");
        assert_eq!(KeyCombo::parse("Shift+Enter").unwrap().to_string(), "Shift+Enter");
        assert_eq!(KeyCombo::parse("shift+enter").unwrap().to_string(), "Shift+Enter");
        assert_eq!(KeyCombo::parse("F12").unwrap().to_string(), "F12");
        assert_eq!(KeyCombo::parse("Ctrl+"), None);
        assert_eq!(KeyCombo::parse("Hyper+Q"), None);
    }

    #[test]
    fn resolver_matches_modifiers_exactly() {
        let sc = Shortcuts::defaults();
        assert_eq!(resolve(&sc, &[key(egui::Key::Enter, false, false)], false), vec![AppAction::Take]);
        assert_eq!(resolve(&sc, &[key(egui::Key::Enter, false, true)], false), vec![AppAction::ProgramStop]);
        assert_eq!(resolve(&sc, &[key(egui::Key::S, true, false)], false), vec![AppAction::Save]);
    }

    #[test]
    fn resolver_ignores_keys_while_typing() {
        let sc = Shortcuts::defaults();
        assert!(resolve(&sc, &[key(egui::Key::Space, false, false)], true).is_empty());
        assert_eq!(resolve(&sc, &[key(egui::Key::S, true, false)], true), vec![AppAction::Save], "Ctrl combos still work while typing");
    }

    #[test]
    fn ctrl_c_arrives_as_a_copy_event() {
        let sc = Shortcuts::defaults();
        assert_eq!(resolve(&sc, &[egui::Event::Copy], false), vec![AppAction::AddColumn]);
        assert!(resolve(&sc, &[egui::Event::Copy], true).is_empty(), "copying text in a field is not Add column");
    }

    #[test]
    fn function_keys_work_while_typing() {
        let sc = Shortcuts::defaults();
        assert_eq!(resolve(&sc, &[key(egui::Key::F12, false, false)], true), vec![AppAction::Panic], "PANIC always works");
    }

    #[test]
    fn corrupt_settings_fall_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("evj-sc-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let f = dir.join("settings.json");
        std::fs::write(&f, b"{ not json").unwrap();
        assert_eq!(Shortcuts::load(&f).map, Shortcuts::defaults().map);
        let mut sc = Shortcuts::defaults();
        sc.set(AppAction::Blackout, KeyCombo::parse("Ctrl+B"));
        sc.save(&f);
        assert_eq!(Shortcuts::load(&f).combo(AppAction::Blackout), KeyCombo::parse("Ctrl+B"));
    }
}
