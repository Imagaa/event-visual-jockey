//! Modular panels: every feature of the window is a dockable tab (egui_dock). The layout is a
//! `layout` section of settings.json.
use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};
use crate::outui::OutputPreviews;
use crate::thumbs::Thumbnailer;
use crate::ui::{Actions, UiState};
use crate::waveform::Waveforms;
use evj_engine::Snapshot;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Panel {
    Grid,
    Program,
    Preview,
    Timeline,
    Properties,
    Outputs,
    Transitions,
    Performance,
    Shortcuts,
}

impl Panel {
    pub const ALL: [Panel; 9] = [
        Panel::Grid,
        Panel::Program,
        Panel::Preview,
        Panel::Timeline,
        Panel::Properties,
        Panel::Outputs,
        Panel::Transitions,
        Panel::Performance,
        Panel::Shortcuts,
    ];
}

pub type Layout = DockState<Panel>;

impl Panel {
    pub fn title(self) -> &'static str {
        match self {
            Panel::Grid => "Grid",
            Panel::Program => "Program",
            Panel::Preview => "Preview",
            Panel::Timeline => "Timeline",
            Panel::Properties => "Properties",
            Panel::Outputs => "Outputs",
            Panel::Transitions => "Transitions",
            Panel::Performance => "Performance",
            Panel::Shortcuts => "Shortcuts",
        }
    }
}

/// The show layout: Grid over Timeline on the left; Program, Preview, Properties on the right.
pub fn preset_live() -> Layout {
    let mut l = DockState::new(vec![Panel::Grid]);
    let tree = l.main_surface_mut();
    let [left, right] = tree.split_right(NodeIndex::root(), 0.70, vec![Panel::Program]);
    tree.split_below(left, 0.75, vec![Panel::Timeline]);
    let [_, below] = tree.split_below(right, 0.34, vec![Panel::Preview]);
    tree.split_below(below, 0.5, vec![Panel::Properties]);
    l
}

/// Preparing a show: the managers open next to Properties.
pub fn preset_setup() -> Layout {
    let mut l = DockState::new(vec![Panel::Grid]);
    let tree = l.main_surface_mut();
    let [left, right] = tree.split_right(NodeIndex::root(), 0.62, vec![Panel::Program, Panel::Preview]);
    tree.split_below(left, 0.72, vec![Panel::Timeline]);
    tree.split_below(right, 0.4, vec![Panel::Properties, Panel::Outputs, Panel::Transitions]);
    l
}

pub fn is_open(l: &Layout, p: Panel) -> bool {
    l.find_tab(&p).is_some()
}

/// Shows `p`: brings its tab to the front, or adds it to the focused (else first / a new) leaf.
pub fn open(l: &mut Layout, p: Panel) {
    match l.find_tab(&p) {
        Some(path) => {
            let _ = l.set_active_tab(path);
            l.set_focused_node_and_surface(path.node_path());
        }
        None => l.push_to_focused_leaf(p),
    }
}

pub fn close(l: &mut Layout, p: Panel) {
    if let Some(path) = l.find_tab(&p) {
        l.remove_tab(path);
    }
}

pub fn toggle(l: &mut Layout, p: Panel) {
    if is_open(l, p) {
        close(l, p);
    } else {
        open(l, p);
    }
}

/// The show must stay controllable: Grid and Program are always there after loading.
pub fn ensure_essentials(l: &mut Layout) {
    for p in [Panel::Grid, Panel::Program] {
        if !is_open(l, p) {
            open(l, p);
        }
    }
}

pub fn to_json(l: &Layout) -> serde_json::Value {
    serde_json::to_value(l).unwrap_or(serde_json::Value::Null)
}

/// A saved layout; anything unreadable (other version, unknown panel) is the Live preset.
pub fn from_json(v: &serde_json::Value) -> Layout {
    let mut v = v.clone();
    unmeasured_points_to_zero(&mut v);
    let mut l: Layout = serde_json::from_value(v).unwrap_or_else(|_| preset_live());
    ensure_essentials(&mut l);
    l
}

/// Rects not measured yet (NaN) are saved as `null`: read them as 0 (they are recomputed on
/// the next draw).
fn unmeasured_points_to_zero(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Object(m) => {
            let point = m.len() == 2 && m.contains_key("x") && m.contains_key("y");
            for (_, x) in m.iter_mut() {
                if point && x.is_null() {
                    *x = serde_json::json!(0.0);
                } else {
                    unmeasured_points_to_zero(x);
                }
            }
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(unmeasured_points_to_zero),
        _ => {}
    }
}

pub fn load(file: &Path) -> Layout {
    let v: Option<serde_json::Value> = std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok());
    v.and_then(|v| v.get("layout").cloned()).map_or_else(preset_live, |v| from_json(&v))
}

/// Saves the layout only when it changed, and not more often than every 2 s.
#[derive(Default)]
pub struct SaveGate {
    last: Option<serde_json::Value>,
    at: Option<f64>,
}

impl SaveGate {
    pub fn due(&mut self, now_json: &serde_json::Value, now: f64) -> bool {
        let changed = self.last.as_ref() != Some(now_json);
        let rested = self.at.is_none_or(|t| now - t >= 2.0);
        if changed && rested {
            (self.last, self.at) = (Some(now_json.clone()), Some(now));
            return true;
        }
        false
    }
}

pub fn save(file: &Path, l: &Layout) {
    crate::shortcuts::write_section(file, "layout", to_json(l));
}

/// Textures the panels show (engine previews shared with the UI device).
pub struct Textures {
    pub program: Option<(egui::TextureId, [f32; 2])>,
    pub transition: Option<(egui::TextureId, [f32; 2])>,
    pub cue: Option<(egui::TextureId, [f32; 2])>,
}

/// Tabs can be closed (and dragged / floated) only while the show is not locked.
pub fn closeable(locked: bool) -> bool {
    !locked
}

struct Viewer<'a> {
    st: &'a mut UiState,
    snap: &'a Snapshot,
    tex: &'a Textures,
    thumbs: &'a mut Thumbnailer,
    waves: &'a mut Waveforms,
    previews: &'a OutputPreviews,
    act: &'a mut Actions,
}

impl egui_dock::TabViewer for Viewer<'_> {
    type Tab = Panel;

    fn title(&mut self, t: &mut Panel) -> egui::WidgetText {
        t.title().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, t: &mut Panel) {
        let (st, snap, act) = (&mut *self.st, self.snap, &mut *self.act);
        match t {
            Panel::Grid => crate::ui::grid(ui, st, snap, self.thumbs, act),
            Panel::Program => crate::ui::program_panel(ui, st, snap, self.tex.program, act),
            Panel::Preview => crate::ui::preview_panel(ui, st, snap, self.tex.cue, act),
            Panel::Timeline => crate::timeline::panel(ui, st, snap, self.waves, self.thumbs, act),
            Panel::Properties => crate::ui::properties_panel(ui, st, snap, self.thumbs, act),
            Panel::Outputs => crate::outui::manager_body(ui, st, act, self.previews),
            Panel::Transitions => crate::trui::manager_body(ui, st, snap, act, self.tex.transition),
            Panel::Performance => crate::outui::performance_body(ui, st, snap),
            Panel::Shortcuts => crate::shortcuts::body(ui, st),
        }
    }

    fn id(&mut self, t: &mut Panel) -> egui::Id {
        egui::Id::new(("evj-panel", t.title()))
    }

    fn closeable(&mut self, _t: &mut Panel) -> bool {
        closeable(self.st.locked)
    }

    fn allowed_in_windows(&self, _t: &mut Panel) -> bool {
        !self.st.locked
    }
}

/// Draws the dock (every open panel) into `ui`.
#[allow(clippy::too_many_arguments)]
pub fn show(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, tex: &Textures, thumbs: &mut Thumbnailer, waves: &mut Waveforms, previews: &OutputPreviews, act: &mut Actions) {
    let mut layout = std::mem::replace(&mut st.layout, DockState::new(vec![]));
    let locked = st.locked;
    egui_dock::DockArea::new(&mut layout)
        .id(egui::Id::new("evj-dock"))
        .style(egui_dock::Style::from_egui(ui.style().as_ref()))
        .show_close_buttons(!locked)
        .draggable_tabs(!locked)
        .show_leaf_collapse_buttons(false)
        .show_leaf_close_all_buttons(false)
        .show_inside(ui, &mut Viewer { st, snap, tex, thumbs, waves, previews, act });
    st.layout = layout;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_set(l: &Layout) -> Vec<Panel> {
        Panel::ALL.into_iter().filter(|p| is_open(l, *p)).collect()
    }

    #[test]
    fn live_preset_shows_the_show_panels() {
        use Panel::*;
        assert_eq!(open_set(&preset_live()), vec![Grid, Program, Preview, Timeline, Properties]);
    }

    #[test]
    fn setup_preset_adds_the_managers() {
        let l = preset_setup();
        for p in [Panel::Grid, Panel::Properties, Panel::Outputs, Panel::Transitions, Panel::Program, Panel::Preview, Panel::Timeline] {
            assert!(is_open(&l, p), "{p:?}");
        }
    }

    #[test]
    fn toggle_closes_and_reopens_and_open_is_idempotent() {
        let mut l = preset_live();
        toggle(&mut l, Panel::Timeline);
        assert!(!is_open(&l, Panel::Timeline));
        toggle(&mut l, Panel::Timeline);
        assert!(is_open(&l, Panel::Timeline));
        open(&mut l, Panel::Outputs);
        open(&mut l, Panel::Outputs);
        assert_eq!(l.iter_all_tabs().filter(|(_, t)| **t == Panel::Outputs).count(), 1);
    }

    #[test]
    fn reopening_into_an_empty_dock_works() {
        let mut l = preset_live();
        for p in Panel::ALL {
            close(&mut l, p);
        }
        open(&mut l, Panel::Preview);
        assert!(is_open(&l, Panel::Preview));
    }

    #[test]
    fn layouts_round_trip() {
        let mut l = preset_setup();
        close(&mut l, Panel::Transitions);
        let back = from_json(&to_json(&l));
        assert_eq!(open_set(&back), open_set(&l));
    }

    #[test]
    fn garbage_or_unknown_layouts_fall_back_to_live() {
        assert_eq!(open_set(&from_json(&serde_json::json!("nonsense"))), open_set(&preset_live()));
        let text = to_json(&preset_live()).to_string().replace("\"Timeline\"", "\"NoSuchPanel\"");
        let v: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(open_set(&from_json(&v)), open_set(&preset_live()));
    }

    #[test]
    fn grid_and_program_always_come_back() {
        let mut l = preset_live();
        close(&mut l, Panel::Grid);
        close(&mut l, Panel::Program);
        let back = from_json(&to_json(&l));
        assert!(is_open(&back, Panel::Grid) && is_open(&back, Panel::Program));
    }

    #[test]
    fn saving_is_throttled_and_only_on_change() {
        let mut s = SaveGate::default();
        let a = to_json(&preset_live());
        assert!(s.due(&a, 0.0), "first time");
        assert!(!s.due(&a, 5.0), "unchanged");
        let b = to_json(&preset_setup());
        assert!(!s.due(&b, 1.5), "changed, but within 2 s of the last save");
        assert!(s.due(&b, 7.1));
        assert!(!s.due(&b, 20.0), "saved already");
    }

    #[test]
    fn saved_next_to_the_other_settings() {
        let f = std::env::temp_dir().join(format!("evj-dock-{}.json", std::process::id()));
        std::fs::write(&f, r#"{"shortcuts":{"Save":"Ctrl+S"}}"#).unwrap();
        let mut l = preset_live();
        close(&mut l, Panel::Properties);
        save(&f, &l);
        assert!(is_open(&load(&f), Panel::Grid));
        assert!(!is_open(&load(&f), Panel::Properties));
        assert!(std::fs::read_to_string(&f).unwrap().contains("Ctrl+S"));
        let _ = std::fs::remove_file(f);
    }
}
