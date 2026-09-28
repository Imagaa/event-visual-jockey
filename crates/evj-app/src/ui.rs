//! Control surface: clip grid (Resolume-style), layer controls, preview, properties, file menu.
use crate::outui::OutputPreviews;
use crate::thumbs::{self, Thumb, Thumbnailer};
use evj_media::convert::HapVariant;
use evj_media::DecoderKind;
use evj_core::keymap::Action;
use evj_core::model::{BlendMode, Clip, FitMode, PlayMode, Project};
use evj_core::transition::resolve_preset;
use evj_engine::{Command, Snapshot};
use egui::{Color32, Rect, RichText, Sense, Stroke, StrokeKind, vec2};
use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Selection {
    None,
    Slot(usize, usize),
    /// A column: all its clips together.
    Scene(usize),
    Layer(usize),
    Composition,
}

/// What the Preview monitor was last told to show.
#[derive(Clone, PartialEq, Debug)]
pub enum Cue {
    Clip(Clip),
    Scene(Vec<Option<Clip>>),
}

#[derive(Clone)]
pub enum Menu {
    New,
    Open,
    Save,
    SaveAs,
    AddClips(usize, usize),
    Relink,
    /// Pick a PPTX / PDF (or a deck.json) for a grid slot.
    ImportPresentation,
    OpenRecent(PathBuf),
    /// Pick an audio file to play with the clip in (layer, column).
    AttachAudio(usize, usize),
    /// Pick a folder; its media fill the grid from layer 1, column 1.
    ImportFolder,
    /// true = load the autosave, false = discard it.
    Recover(bool),
}

pub struct UiState {
    pub project: Project,
    pub project_path: Option<PathBuf>,
    pub dirty: bool,
    pub selected: Selection,
    /// Per layer: (deck, column) of the clip triggered last.
    pub playing: Vec<Option<(usize, usize)>>,
    pub output_open: bool,
    pub status: String,
    /// Slot rectangles of the last frame (drop target lookup).
    pub slot_rects: Vec<(Rect, usize, usize)>,
    /// Slot under the cursor while files are dragged over the window.
    pub drop_target: Option<(usize, usize)>,
    /// Media the opened project points to but that is not on disk.
    pub missing: Vec<PathBuf>,
    /// Autosave left behind by a session that did not exit cleanly.
    pub recovery: Option<PathBuf>,
    /// Key-mapping mode: clicks pick an action, the next key press binds it.
    pub map_keys: bool,
    map_target: Option<Action>,
    res_edit: (u32, u32),
    /// Transition preset for the next trigger (overrides layer defaults).
    pub next_transition: Option<String>,
    pub selected_preset: usize,
    /// Transition the engine is previewing for the manager.
    pub previewing: Option<String>,
    logo: Option<egui::TextureHandle>,
    /// Monitor names Windows currently reports.
    pub monitors: Vec<String>,
    /// (monitor, GPU driving it), and the GPU the show renders on — for the cross-GPU warning.
    pub display_gpus: Vec<(String, String)>,
    pub engine_gpu: String,
    /// Running background jobs (status bar).
    pub jobs: Vec<String>,
    /// Quit was requested while outputs are live.
    pub confirm_quit: bool,
    /// Audio output devices (refreshed when the Composition panel opens).
    pub audio_devices: Vec<(String, u16)>,
    /// Sound outputs of this laptop (settings.json); `audio_dirty` = main saves them.
    pub audio: crate::audioset::AudioSettings,
    pub audio_dirty: bool,
    /// The last Preview choice clashed with Program and was refused.
    pub audio_refused: bool,
    pub preview_holds: [crate::meters::Hold; 2],
    pub master_volume: f32,
    pub present: crate::present::PresentState,
    /// Clip on the Preview monitor (what the engine was last told to cue).
    pub cue_sent: Option<Cue>,
    /// Recently opened shows (newest first) and the welcome window.
    pub recent: Vec<PathBuf>,
    pub show_welcome: bool,
    /// App-wide shortcuts (settings.json) and their editor state.
    pub shortcuts: crate::shortcuts::Shortcuts,
    /// The dock: which panels are open and where (settings.json `layout`).
    pub layout: crate::dock::Layout,
    pub shortcut_armed: Option<crate::shortcuts::AppAction>,
    /// LOCK LIVE and its hold-to-unlock state.
    pub locked: bool,
    pub unlock: crate::lock::Unlock,
    /// Peak-hold state of each layer's mini meter.
    pub layer_holds: Vec<[crate::meters::Hold; 2]>,
    pub master_holds: [crate::meters::Hold; 2],
    /// BLACKOUT is on (outputs fading / faded to black).
    pub blackout: bool,
    /// The Preview clip is paused (from the last snapshot).
    pub cue_paused: bool,
    /// Status-bar save state: last successful save (hh:mm) and a failed autosave.
    pub last_saved: Option<String>,
    pub autosave_error: Option<String>,
    /// New / Open waiting for the unsaved-changes prompt.
    pub confirm_discard: Option<Menu>,
    /// Recent shows as welcome-screen cards (refreshed when the welcome opens).
    pub recent_cards: Vec<crate::welcome::RecentCard>,
    /// Timeline: seek the Program (true) or the Preview; the drag in progress; last seek sent
    /// (clip seconds, and when: UI time).
    pub seek_program: bool,
    pub tl_drag: Option<crate::timeline::Drag>,
    pub tl_last_seek: f64,
    pub tl_last_seek_at: f64,
    /// I / O pressed: set start (true) / end (false) once the snapshot is at hand.
    pub pending_mark: Option<bool>,
    /// Per layer: the sequence playing on it.
    pub runs: Vec<Option<crate::sequence::SeqRun>>,
    /// Shift+click multi-selection (slots of one layer).
    pub multi: Vec<(usize, usize)>,
    /// Per layer: the engine's `queue_taken` of the last snapshot.
    pub last_taken: Vec<u64>,
    pub last_failed: Vec<u64>,
    /// Scene being renamed: (column, name typed so far).
    pub rename_scene: Option<(usize, String)>,
    /// Presentations being imported: source file → (deck, layer, column) of its slot.
    pub importing: std::collections::HashMap<PathBuf, (usize, usize, usize)>,
    /// Imports that failed (source file → error), shown on the slot.
    pub import_errors: std::collections::HashMap<PathBuf, String>,
}

#[derive(Default)]
pub struct Actions {
    pub commands: Vec<Command>,
    pub toggle_output: bool,
    pub menu: Option<Menu>,
    /// Engine composition was resized: the preview must be re-opened.
    pub reopen_preview: bool,
    /// Output settings edited: push them to the output windows.
    pub outputs_changed: bool,
    /// Show numbered test patterns on every output for a few seconds.
    pub identify: bool,
    pub convert: Option<(PathBuf, HapVariant)>,
    pub quit: bool,
    /// App actions main.rs finishes (New / Open / Save / Save As).
    pub app: Vec<crate::shortcuts::AppAction>,
    /// "Save and quit" from the quit prompt.
    pub save_and_quit: bool,
    /// Answer to the unsaved-changes prompt: Some(true) = save first, Some(false) = discard.
    pub discard_answer: Option<bool>,
    /// Import this presentation again (it failed).
    pub retry_import: Option<PathBuf>,
}

impl Actions {
    /// Adds a later egui pass's actions. egui may run the UI twice in one frame (a discarded
    /// sizing pass); only the first pass sees input, but state changes (e.g. the Preview cue)
    /// happen in whichever pass notices them — so every pass's commands must be kept.
    pub fn absorb(&mut self, o: Actions) {
        self.commands.extend(o.commands);
        self.toggle_output |= o.toggle_output;
        if o.menu.is_some() {
            self.menu = o.menu;
        }
        self.reopen_preview |= o.reopen_preview;
        self.outputs_changed |= o.outputs_changed;
        self.identify |= o.identify;
        if o.convert.is_some() {
            self.convert = o.convert;
        }
        self.quit |= o.quit;
        self.app.extend(o.app);
        self.save_and_quit |= o.save_and_quit;
        if o.discard_answer.is_some() {
            self.discard_answer = o.discard_answer;
        }
        if o.retry_import.is_some() {
            self.retry_import = o.retry_import;
        }
    }
}

impl UiState {
    pub fn new(project: Project) -> UiState {
        let n = project.composition.layers.len();
        let res = (project.composition.width, project.composition.height);
        UiState {
            project,
            project_path: None,
            dirty: false,
            selected: Selection::None,
            playing: vec![None; n],
            output_open: false,
            status: String::new(),
            slot_rects: Vec::new(),
            drop_target: None,
            missing: Vec::new(),
            recovery: None,
            map_keys: false,
            map_target: None,
            res_edit: res,
            next_transition: None,
            selected_preset: 1,
            previewing: None,
            logo: None,
            monitors: Vec::new(),
            display_gpus: Vec::new(),
            engine_gpu: String::new(),
            jobs: Vec::new(),
            confirm_quit: false,
            audio_devices: Vec::new(),
            audio: Default::default(),
            audio_dirty: false,
            audio_refused: false,
            preview_holds: Default::default(),
            master_volume: 1.0,
            present: Default::default(),
            cue_sent: None,
            recent: Vec::new(),
            show_welcome: false,
            shortcuts: crate::shortcuts::Shortcuts::defaults(),
            shortcut_armed: None,
            layout: crate::dock::preset_live(),
            locked: false,
            unlock: Default::default(),
            layer_holds: Vec::new(),
            master_holds: Default::default(),
            blackout: false,
            cue_paused: false,
            last_saved: None,
            autosave_error: None,
            confirm_discard: None,
            recent_cards: Vec::new(),
            seek_program: false,
            tl_drag: None,
            tl_last_seek: -1.0,
            tl_last_seek_at: f64::NEG_INFINITY,
            pending_mark: None,
            runs: vec![None; n],
            multi: Vec::new(),
            last_taken: Vec::new(),
            last_failed: Vec::new(),
            rename_scene: None,
            importing: Default::default(),
            import_errors: Default::default(),
        }
    }

    /// Commands that bring a (new) engine in line with the project.
    pub fn sync_engine(&self) -> Vec<Command> {
        let c = &self.project.composition;
        let mut v = vec![
            Command::SetResolution { width: c.width, height: c.height },
            Command::SetLayerCount(c.layers.len()),
            Command::SetBpm(c.bpm),
            Command::SetCompositionEffects(c.effects.clone()),
            Command::CueClip(None), // a new show starts with an empty Preview
        ];
        for (i, l) in c.layers.iter().enumerate() {
            v.push(Command::Clear { layer: i, transition: None });
            v.push(Command::SetLayer { layer: i, props: l.clone() });
        }
        v
    }

    pub fn replace_project(&mut self, project: Project, path: Option<PathBuf>, act: &mut Actions) {
        let recent = std::mem::take(&mut self.recent);
        let shortcuts = self.shortcuts.clone();
        // Engine-side states the new show does not reset: lock, blackout, master level.
        let (locked, blackout, master_volume) = (self.locked, self.blackout, self.master_volume);
        let layout = std::mem::replace(&mut self.layout, crate::dock::preset_live());
        let audio = self.audio.clone();
        *self = UiState { project_path: path, output_open: self.output_open, recent, shortcuts, locked, blackout, master_volume, layout, audio, ..UiState::new(project) };
        act.commands.extend(self.sync_engine());
        act.reopen_preview = true;
    }

    /// PANIC: every layer off, the panic media full-frame on layer 1, all sound faded out.
    pub fn panic(&mut self, act: &mut Actions) {
        self.runs.iter_mut().for_each(|r| *r = None);
        for layer in 0..self.project.composition.layers.len() {
            act.commands.push(Command::Clear { layer, transition: None });
            if let Some(p) = self.playing.get_mut(layer) {
                *p = None;
            }
        }
        let (d, l, c) = self.project.panic_media.unwrap_or((0, 0, 0));
        if let Some(clip) = self.project.decks.get(d).and_then(|deck| deck.clip(l, c)).cloned() {
            act.commands.push(Command::Trigger { layer: 0, clip, transition: None });
        }
        act.commands.push(Command::PanicAudio);
        act.commands.push(Command::Blackout(false));
        self.blackout = false;
        self.master_volume = 0.0;
        self.audio.preview_volume = 0.0; // the engine muted the Preview too
        self.audio_dirty = true;
        self.status = "PANIC — layers cleared, sound faded out. Raise the master volume to continue.".into();
    }

    pub fn run_app_action(&mut self, a: crate::shortcuts::AppAction, act: &mut Actions) {
        use crate::shortcuts::AppAction::*;
        let content = crate::lock::allowed(self.locked, crate::lock::Op::Content);
        match a {
            PreviewPlayPause => act.commands.push(Command::CuePause(!self.cue_paused)),
            PreviewStop => act.commands.push(Command::CueRewind),
            Take => match self.selected {
                Selection::Slot(l, c) => self.trigger(l, c, act),
                Selection::Scene(c) => self.trigger_column(c, act),
                _ => {}
            },
            ProgramStop => {
                for l in 0..self.project.composition.layers.len() {
                    self.clear(l, act);
                }
            }
            Blackout => {
                self.blackout = !self.blackout;
                act.commands.push(Command::Blackout(self.blackout));
            }
            Panic => self.panic(act),
            LockToggle => {
                if self.locked {
                    self.status = "Hold the 🔒 button 1 s to unlock".into();
                } else {
                    self.locked = true;
                    self.status = "LOCKED — show control only".into();
                }
            }
            TapTempo => act.commands.push(Command::Tap),
            AddColumn if content => {
                self.project.add_column();
                self.dirty = true;
            }
            AddLayer if content => {
                self.project.add_layer();
                self.layers_changed(act);
            }
            ShowShortcuts => crate::dock::toggle(&mut self.layout, crate::dock::Panel::Shortcuts),
            ShowPerf => crate::dock::toggle(&mut self.layout, crate::dock::Panel::Performance),
            ShowOutputs => crate::dock::toggle(&mut self.layout, crate::dock::Panel::Outputs),
            MarkIn => self.pending_mark = Some(true),
            MarkOut => self.pending_mark = Some(false),
            SeqNext => {
                // The selected slot's layer, else the top layer that runs a sequence.
                let sel = match self.selected {
                    Selection::Slot(l, _) => Some(l).filter(|l| self.runs.get(*l).is_some_and(Option::is_some)),
                    _ => None,
                };
                if let Some(l) = sel.or_else(|| (0..self.runs.len()).rev().find(|l| self.runs[*l].is_some())) {
                    crate::sequence::next_now(self, l, act);
                }
            }
            New | Open if !content => self.status = "Locked: unlock to open or create a show".into(),
            New | Open | Save | SaveAs => act.app.push(a),
            AddColumn | AddLayer => self.status = "Locked: unlock to add columns or layers".into(),
        }
    }

    /// The clips of column `col`, one per layer.
    pub fn scene_clips(&self, col: usize) -> Vec<Option<Clip>> {
        (0..self.project.composition.layers.len()).map(|l| self.project.deck().and_then(|d| d.clip(l, col)).cloned()).collect()
    }

    /// The Preview monitor follows the selection: a slot, or a whole scene (edits included).
    pub fn sync_cue(&mut self, act: &mut Actions) {
        let want = match self.selected {
            Selection::Slot(l, c) => self.project.deck().and_then(|d| d.clip(l, c)).cloned().map(Cue::Clip),
            Selection::Scene(c) => Some(Cue::Scene(self.scene_clips(c))),
            _ => None,
        };
        if want == self.cue_sent {
            return;
        }
        act.commands.push(match &want {
            Some(Cue::Clip(c)) => Command::CueClip(Some(c.clone())),
            Some(Cue::Scene(v)) => Command::CueScene(v.clone()),
            None => Command::CueClip(None),
        });
        self.cue_sent = want;
    }

    pub fn logo_texture(&self) -> Option<egui::TextureId> {
        self.logo.as_ref().map(|t| t.id())
    }

    /// Shows the start screen with fresh recent-show cards.
    pub fn open_welcome(&mut self) {
        self.recent_cards = crate::welcome::cards(&self.recent);
        self.show_welcome = true;
    }

    pub fn trigger(&mut self, layer: usize, col: usize, act: &mut Actions) {
        let deck = self.project.active_deck;
        if let Some(clip) = self.project.deck().and_then(|d| d.clip(layer, col)).cloned() {
            let layer_props = self.project.composition.layers.get(layer).cloned().unwrap_or_default();
            let transition = resolve_preset(&self.project.transitions, &clip, self.next_transition.as_deref(), &layer_props).cloned();
            let had_run = self.runs.get(layer).is_some_and(Option::is_some);
            let clip = crate::sequence::start(self, layer, col).unwrap_or(clip);
            act.commands.push(Command::Trigger { layer, clip, transition });
            // After the Trigger: a Trigger drops the engine's queue.
            if self.runs.get(layer).is_some_and(Option::is_some) {
                crate::sequence::queue(self, layer, act);
            } else if had_run {
                act.commands.push(Command::QueueNext { layer, clip: None });
            }
            if let Some(p) = self.playing.get_mut(layer) {
                *p = Some((deck, col));
            }
            if let Some(c) = self.project.deck_mut().and_then(|d| d.clip_mut(layer, col)).filter(|c| !c.aired) {
                c.aired = true;
                self.dirty = true;
            }
        }
    }

    /// A column is a scene: layers without a clip in it are cleared.
    pub fn trigger_column(&mut self, col: usize, act: &mut Actions) {
        for layer in 0..self.project.composition.layers.len() {
            if self.project.deck().and_then(|d| d.clip(layer, col)).is_some() {
                self.trigger(layer, col, act);
            } else {
                self.clear(layer, act);
            }
        }
    }

    /// Empties a layer, fading out with the next / layer-default transition.
    pub fn clear(&mut self, layer: usize, act: &mut Actions) {
        let layer_props = self.project.composition.layers.get(layer).cloned().unwrap_or_default();
        let transition = resolve_preset(&self.project.transitions, &Clip::default(), self.next_transition.as_deref(), &layer_props).cloned();
        act.commands.push(Command::Clear { layer, transition });
        if let Some(p) = self.playing.get_mut(layer) {
            *p = None;
        }
        if let Some(r) = self.runs.get_mut(layer) {
            *r = None;
        }
    }

    pub fn run(&mut self, a: &Action, act: &mut Actions) {
        match *a {
            Action::TriggerSlot { layer, col } => self.trigger(layer, col, act),
            Action::TriggerColumn(col) => self.trigger_column(col, act),
            Action::ClearLayer(layer) => self.clear(layer, act),
            Action::ClearAll => {
                for l in 0..self.project.composition.layers.len() {
                    self.clear(l, act);
                }
            }
            Action::TapTempo => act.commands.push(Command::Tap),
        }
    }

    /// In map mode a click chooses the action to bind instead of running it.
    fn click(&mut self, a: Action, act: &mut Actions) {
        if self.map_keys {
            self.status = format!("Press a key for {a:?} (Esc removes the binding)");
            self.map_target = Some(a);
        } else {
            self.run(&a, act);
        }
    }

    fn key_label(&self, a: &Action) -> Option<String> {
        self.project.keymap.key_for(a).map(str::to_string)
    }

    /// Puts clips into the grid; returns the PowerPoint / PDF files among them, which still
    /// have to be imported (their slots show "Importing…" meanwhile).
    pub fn place(&mut self, layer: usize, col: usize, paths: &[PathBuf]) -> Vec<PathBuf> {
        let deck = self.project.active_deck;
        let Some(d) = self.project.deck_mut() else { return Vec::new() };
        let slots = d.place(layer, col, paths);
        self.dirty = true;
        let mut todo = Vec::new();
        for (p, (l, c)) in paths.iter().zip(slots) {
            if evj_core::slides::is_presentation(p) {
                self.importing.insert(p.clone(), (deck, l, c));
                self.import_errors.remove(p);
                todo.push(p.clone());
            }
        }
        todo
    }

    fn layers_changed(&mut self, act: &mut Actions) {
        let n = self.project.composition.layers.len();
        self.playing.resize(n, None);
        self.runs = vec![None; n];
        act.commands.push(Command::SetLayerCount(n));
        for (i, l) in self.project.composition.layers.iter().enumerate() {
            act.commands.push(Command::SetLayer { layer: i, props: l.clone() });
        }
        self.dirty = true;
    }

    pub(crate) fn is_playing(&self, layer: usize, col: usize) -> bool {
        self.playing.get(layer).copied().flatten() == Some((self.project.active_deck, col))
    }
}

fn time(secs: f64) -> String {
    let s = secs.max(0.0);
    format!("{}:{:04.1}", (s / 60.0) as u32, s % 60.0)
}

pub fn draw(
    ui: &mut egui::Ui,
    st: &mut UiState,
    snap: &Snapshot,
    preview: Option<(egui::TextureId, [f32; 2])>,
    tr_preview: Option<(egui::TextureId, [f32; 2])>,
    cue_preview: Option<(egui::TextureId, [f32; 2])>,
    thumbs: &mut Thumbnailer,
    waves: &mut crate::waveform::Waveforms,
    out_previews: &OutputPreviews,
) -> Actions {
    let mut act = Actions::default();
    st.present.hover_preview = false;
    st.cue_paused = snap.cue_state.as_ref().is_some_and(|c| c.paused);
    let typing = ui.ctx().egui_wants_keyboard_input();
    let presenting = st.present.active();
    let events = ui.input(|i| i.events.clone());
    if st.shortcut_armed.is_none() && !st.map_keys {
        for a in crate::shortcuts::resolve(&st.shortcuts, &events, typing) {
            // A running presentation owns the clicker keys (Space, Enter, B, arrows …).
            let clicker = presenting && st.shortcuts.combo(a).is_some_and(|c| !c.ctrl && !c.alt && crate::present::is_clicker_key(c.key));
            if !clicker {
                st.run_app_action(a, &mut act);
            }
        }
    }
    keys(ui.ctx(), st, &mut act);
    if let Some(start) = st.pending_mark.take() {
        crate::timeline::mark(st, snap, start, &mut act);
    }
    crate::present::update(st, snap, &mut act);
    crate::sequence::update(st, snap, &mut act);
    st.sync_cue(&mut act);
    top_bar(ui, st, snap, &mut act);
    egui::Panel::bottom("status").show(ui, |ui| {
        ui.horizontal(|ui| {
            let chip = |ui: &mut egui::Ui, text: String, color: Color32| {
                egui::Frame::new().fill(color.gamma_multiply(0.25)).corner_radius(3.0).inner_margin(egui::Margin::symmetric(6, 1)).show(ui, |ui| {
                    ui.label(RichText::new(text).small().color(color));
                });
            };
            match (&st.autosave_error, st.dirty, &st.last_saved) {
                (Some(e), _, _) => chip(ui, format!("Autosave failed: {e}"), Color32::from_rgb(235, 60, 60)),
                (None, true, _) => chip(ui, "Unsaved".into(), Color32::from_rgb(235, 190, 50)),
                (None, false, Some(t)) => chip(ui, format!("Saved {t}"), Color32::from_rgb(70, 200, 110)),
                (None, false, None) => chip(ui, if st.project_path.is_some() { "Saved".into() } else { "New show".into() }, Color32::from_gray(160)),
            }
            if st.project_path.is_some() {
                chip(ui, "Autosave on".into(), Color32::from_gray(160));
            }
            let live = if st.output_open { Color32::from_rgb(235, 60, 60) } else { Color32::from_gray(160) };
            chip(ui, format!("Outputs {}", if st.output_open { "LIVE" } else { "off" }), live);
            if st.locked {
                chip(ui, "LOCKED".into(), Color32::from_rgb(255, 170, 60));
            }
            match (&snap.preview_audio_device, &snap.preview_audio_error) {
                (Some(d), _) => {
                    let pair = st.audio.preview.as_ref().map(|r| crate::audioset::pair_label(r.first_channel)).unwrap_or_default();
                    chip(ui, format!("🎧 {d} {pair}"), Color32::from_rgb(60, 200, 90));
                }
                (None, Some(e)) => chip(ui, format!("Preview audio off: {e}"), Color32::from_rgb(235, 60, 60)),
                _ => {}
            }
            ui.label(&st.status);
            for j in &st.jobs {
                ui.separator();
                ui.label(j);
            }
        });
    });
    crate::trui::sync_preview(st, &mut act);
    if !crate::dock::is_open(&st.layout, crate::dock::Panel::Shortcuts) {
        st.shortcut_armed = None;
    }
    egui::CentralPanel::default().show(ui, |ui| {
        if st.show_welcome {
            crate::welcome::screen(ui, st, thumbs, &mut act);
        } else {
            let tex = crate::dock::Textures { program: preview, transition: tr_preview, cue: cue_preview };
            crate::dock::show(ui, st, snap, &tex, thumbs, waves, out_previews, &mut act);
        }
    });
    dialogs(ui.ctx(), st, &mut act);
    act
}

fn keys(ctx: &egui::Context, st: &mut UiState, act: &mut Actions) {
    if ctx.egui_wants_keyboard_input() || st.shortcut_armed.is_some() {
        return;
    }
    // Ctrl / Alt combos belong to the app shortcuts, never to the project keymap.
    let pressed: Vec<egui::Key> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key { key, pressed: true, repeat: false, modifiers, .. } if !modifiers.ctrl && !modifiers.alt => Some(*key),
                _ => None,
            })
            .collect()
    });
    for key in pressed {
        // While presenting the clicker path (raw input) owns these keys.
        if st.present.active() && crate::present::is_clicker_key(key) && st.map_target.is_none() {
            continue;
        }
        if let Some(target) = st.map_target.take() {
            if key == egui::Key::Escape {
                if let Some(k) = st.project.keymap.key_for(&target).map(str::to_string) {
                    st.project.keymap.unbind(&k);
                }
                st.status = format!("Removed key for {target:?}");
            } else if let Some(taken) = st.shortcuts.plain_key(key) {
                st.status = format!("{} is the {} shortcut — pick another key", key.name(), taken.label());
                st.map_target = Some(target);
                continue;
            } else {
                st.project.keymap.bind(key.name(), target.clone());
                st.status = format!("{} → {target:?}", key.name());
            }
            st.dirty = true;
        } else if !st.map_keys && st.shortcuts.plain_key(key).is_none() {
            if let Some(a) = st.project.keymap.resolve(key.name()).cloned() {
                st.run(&a, act);
            }
        }
    }
}

fn dialogs(ctx: &egui::Context, st: &mut UiState, act: &mut Actions) {
    if st.confirm_quit {
        egui::Window::new("Quit EVJ?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            if st.output_open {
                ui.label("The audience screens will go black.");
            }
            if st.dirty {
                ui.label("The show has unsaved changes.");
            }
            ui.horizontal(|ui| {
                if st.dirty && ui.button(RichText::new("Save and quit").strong()).clicked() {
                    act.save_and_quit = true;
                    st.confirm_quit = false;
                }
                if ui.button(if st.dirty { "Quit without saving" } else { "Quit" }).clicked() {
                    act.quit = true;
                }
                if ui.button("Cancel").clicked() {
                    st.confirm_quit = false;
                }
            });
        });
    }
    if st.confirm_discard.is_some() {
        egui::Window::new("Unsaved changes").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("Save the current show first?");
            ui.horizontal(|ui| {
                if ui.button(RichText::new("Save").strong()).clicked() {
                    act.discard_answer = Some(true);
                }
                if ui.button("Don't save").clicked() {
                    act.discard_answer = Some(false);
                }
                if ui.button("Cancel").clicked() {
                    st.confirm_discard = None;
                }
            });
        });
    }
    if st.recovery.is_some() {
        egui::Window::new("Recover show?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("EVJ did not close normally last time. Recover the autosaved show?");
            ui.horizontal(|ui| {
                if ui.button("Recover").clicked() {
                    act.menu = Some(Menu::Recover(true));
                }
                if ui.button("Discard").clicked() {
                    act.menu = Some(Menu::Recover(false));
                }
            });
        });
    }
    if let Some((c, mut name)) = st.rename_scene.clone() {
        let mut done = false;
        egui::Window::new("Rename scene").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            let r = ui.text_edit_singleline(&mut name);
            r.request_focus();
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                if ui.button("OK").clicked() || enter {
                    if let Some(d) = st.project.deck_mut() {
                        d.set_scene_name(c, &name);
                    }
                    st.dirty = true;
                    done = true;
                }
                if ui.button("Cancel").clicked() {
                    done = true;
                }
            });
        });
        st.rename_scene = if done { None } else { Some((c, name)) };
    }
    if !st.missing.is_empty() {
        let mut open = true;
        egui::Window::new(format!("Missing media ({})", st.missing.len())).open(&mut open).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                for m in &st.missing {
                    ui.label(m.to_string_lossy());
                }
            });
            if ui.button("Relink from folder…").clicked() {
                act.menu = Some(Menu::Relink);
            }
        });
        if !open {
            st.missing.clear();
        }
    }
}

fn top_bar(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, act: &mut Actions) {
    egui::Panel::top("top").show(ui, |ui| {
        ui.horizontal(|ui| {
            let logo = st.logo.get_or_insert_with(|| {
                let img = egui::ColorImage::from_rgba_unmultiplied([160, 103], include_bytes!("../../../assets/evj-logo-160x103.rgba"));
                ui.ctx().load_texture("evj-logo", img, egui::TextureOptions::LINEAR)
            });
            ui.image((logo.id(), vec2(34.0, 22.0)));
            ui.menu_button("File", |ui| {
                let content = crate::lock::allowed(st.locked, crate::lock::Op::Content);
                for (label, m) in [("New", Menu::New), ("Open…", Menu::Open), ("Save", Menu::Save), ("Save As…", Menu::SaveAs), ("Relink missing media…", Menu::Relink)] {
                    let ok = content || matches!(m, Menu::Save | Menu::SaveAs);
                    if ui.add_enabled(ok, egui::Button::new(label)).clicked() {
                        act.menu = Some(m);
                        ui.close();
                    }
                }
                if ui.add_enabled(content, egui::Button::new("Import presentation…")).on_hover_text("PowerPoint or PDF into a grid slot").clicked() {
                    act.menu = Some(Menu::ImportPresentation);
                    ui.close();
                }
                if ui.button("Shortcuts…  (F1)").clicked() {
                    crate::dock::open(&mut st.layout, crate::dock::Panel::Shortcuts);
                    ui.close();
                }
                ui.menu_button("Open Recent", |ui| {
                    if st.recent.is_empty() {
                        ui.label("(none yet)");
                    }
                    for p in st.recent.clone() {
                        let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                        if ui.add_enabled(p.exists(), egui::Button::new(name)).on_hover_text(p.to_string_lossy()).clicked() {
                            act.menu = Some(Menu::OpenRecent(p));
                            ui.close();
                        }
                    }
                });
                if ui.button("Welcome screen").clicked() {
                    st.open_welcome();
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                for p in crate::dock::Panel::ALL {
                    let mut on = crate::dock::is_open(&st.layout, p);
                    if ui.checkbox(&mut on, p.title()).changed() {
                        crate::dock::toggle(&mut st.layout, p);
                    }
                }
                ui.separator();
                if ui.button("Live layout").on_hover_text("Grid + Timeline, Program, Preview, Properties").clicked() {
                    st.layout = crate::dock::preset_live();
                    ui.close();
                }
                if ui.button("Setup layout").on_hover_text("Properties, Outputs and Transitions open for preparing a show").clicked() {
                    st.layout = crate::dock::preset_setup();
                    ui.close();
                }
                if ui.button("Reset layout").clicked() {
                    st.layout = crate::dock::preset_live();
                    ui.close();
                }
            });
            ui.separator();
            let name = st.project_path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned());
            ui.strong(format!("{}{}", name.as_deref().unwrap_or("Untitled"), if st.dirty { " *" } else { "" }));
            ui.separator();
            if ui.toggle_value(&mut st.map_keys, "⌨ Map keys").changed() {
                st.map_target = None;
                st.status = if st.map_keys { "Map keys: click a slot, a column header or a layer's ✖, then press a key".into() } else { String::new() };
            }
            let live = egui::RichText::new(if st.output_open { "LIVE — stop" } else { "Go live" });
            let live = if st.output_open { live.color(Color32::from_rgb(255, 90, 90)) } else { live };
            if ui.button(live).on_hover_text("Open / close all output screens").clicked() {
                act.toggle_output = true;
            }
            if ui.button("Outputs…").clicked() {
                crate::dock::toggle(&mut st.layout, crate::dock::Panel::Outputs);
            }
            if ui.button("Perf").clicked() {
                crate::dock::toggle(&mut st.layout, crate::dock::Panel::Performance);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let panic = egui::Button::new(RichText::new("PANIC").strong().color(Color32::WHITE)).fill(Color32::from_rgb(200, 30, 30)).min_size(vec2(84.0, 26.0));
                let panic_key = st.shortcuts.combo(crate::shortcuts::AppAction::Panic).map(|c| format!(" ({c})")).unwrap_or_default();
                if ui.add(panic).on_hover_text(format!("Clear every layer, play the panic media, fade sound out{panic_key}")).clicked() {
                    st.panic(act);
                }
                let on = st.blackout;
                let bo = egui::Button::new(RichText::new(if on { "BLACKOUT ON" } else { "BLACKOUT" }).strong())
                    .fill(if on { Color32::from_rgb(15, 15, 15) } else { Color32::from_gray(60) })
                    .stroke(Stroke::new(if on { 2.0 } else { 0.0 }, Color32::from_rgb(235, 60, 60)));
                let bo_key = st.shortcuts.combo(crate::shortcuts::AppAction::Blackout).map(|c| format!(" ({c})")).unwrap_or_default();
                if ui.add(bo).on_hover_text(format!("Fade all outputs to black / back{bo_key}")).clicked() {
                    st.run_app_action(crate::shortcuts::AppAction::Blackout, act);
                }
                let now = ui.input(|i| i.time);
                crate::lock::toggle_button(ui, st, now);
            });
        });
        ui.horizontal(|ui| {
            ui.label("🔊");
            if ui.add(egui::Slider::new(&mut st.master_volume, 0.0..=1.0).show_value(false)).on_hover_text("Master volume").changed() {
                act.commands.push(Command::SetMasterVolume(st.master_volume));
            }
            ui.separator();
            let tap_key = st.key_label(&Action::TapTempo);
            if st.map_keys {
                if ui.button(format!("TAP key{}", tap_key.map(|k| format!(" [{k}]")).unwrap_or_default())).clicked() {
                    st.click(Action::TapTempo, act);
                }
            } else if let Some(b) = crate::fxui::bpm(ui, snap, &mut act.commands) {
                st.project.composition.bpm = b;
                st.dirty = true;
            }
            ui.separator();
            crate::trui::next_bar(ui, st);
            ui.separator();
            let fps_color = if snap.fps > 0.0 && snap.fps < 55.0 { Color32::LIGHT_RED } else { ui.visuals().text_color() };
            ui.colored_label(fps_color, format!("{:.1} fps", snap.fps));
            ui.label(format!("p99 {:.1} ms", snap.p99_ms));
            ui.label(format!("dropped {}", snap.dropped));
            ui.label(format!("{}×{}", snap.resolution.0, snap.resolution.1));
        });
    });
}

/// A monitor picture at full panel width (black box while there is nothing to show).
fn monitor_image(ui: &mut egui::Ui, tex: Option<(egui::TextureId, [f32; 2])>, show: bool) {
    let w = ui.available_width();
    let aspect = tex.map_or(16.0 / 9.0, |(_, s)| s[0] / s[1].max(1.0));
    let size = vec2(w, w / aspect);
    match tex.filter(|_| show) {
        Some((id, _)) => {
            ui.image((id, size));
        }
        None => {
            let (r, _) = ui.allocate_exact_size(size, Sense::hover());
            ui.painter().rect_filled(r, 2.0, Color32::BLACK);
        }
    }
}

/// Small filled circle (tally light) — the default fonts have no U+25CF.
fn dot(ui: &mut egui::Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
    ui.painter().circle_filled(r.center(), 4.5, color);
}

/// The Program panel: what the audience sees, the master meter, and every layer on air.
pub(crate) fn program_panel(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, program: Option<(egui::TextureId, [f32; 2])>, act: &mut Actions) {
    ui.horizontal(|ui| {
        dot(ui, Color32::from_rgb(235, 60, 60));
        ui.label(RichText::new("PROGRAM").strong().color(Color32::from_rgb(235, 60, 60)));
        ui.label(RichText::new("what the audience sees").small().weak());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::meters::clip_lamp(ui, snap.master_clip) {
                act.commands.push(Command::ResetClip);
            }
        });
    });
    let now = ui.input(|i| i.time);
    ui.horizontal(|ui| {
        let w = ui.available_width() - 26.0;
        let aspect = program.map_or(16.0 / 9.0, |(_, s)| s[0] / s[1].max(1.0));
        ui.allocate_ui(vec2(w, w / aspect), |ui| {
            monitor_image(ui, program, true);
            if snap.blackout > 0.0 {
                let r = ui.min_rect();
                ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha((snap.blackout * 255.0) as u8));
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "BLACKOUT", egui::FontId::proportional(18.0), Color32::from_rgb(235, 60, 60));
            }
        });
        ui.vertical(|ui| {
            crate::meters::vmeter(ui, snap.master_peaks, &mut st.master_holds, now, (w / aspect - 40.0).max(40.0));
        });
    });
    crate::transport::program_strip(ui, st, snap, act);
}

/// The Preview panel: the cued clip / scene, TAKE, its transport and its sound.
pub(crate) fn preview_panel(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, cue: Option<(egui::TextureId, [f32; 2])>, act: &mut Actions) {
    let cued = match st.selected {
        Selection::Slot(l, c) => st.project.deck().and_then(|d| d.clip(l, c)).map(|clip| format!("{} · layer {}", clip.name, l + 1)),
        Selection::Scene(c) => {
            let n = st.scene_clips(c).iter().flatten().count();
            (n > 0).then(|| format!("{} · {n} layer(s)", st.project.deck().map(|d| d.scene_name(c)).unwrap_or_default()))
        }
        _ => None,
    };
    ui.horizontal(|ui| {
        dot(ui, Color32::from_rgb(60, 200, 90));
        ui.label(RichText::new("PREVIEW").strong().color(Color32::from_rgb(60, 200, 90)));
        match &cued {
            Some(text) => ui.label(RichText::new(text).small()),
            None => ui.label(RichText::new("click a clip or a scene to cue it").small().weak()),
        };
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let take_key = st.shortcuts.combo(crate::shortcuts::AppAction::Take).map(|c| format!(" ({c})")).unwrap_or_default();
            let take = ui.add_enabled(cued.is_some(), egui::Button::new(RichText::new("TAKE ▶").strong())).on_hover_text(format!("Put the Preview on Program (with the next / layer transition){take_key}"));
            if take.clicked() {
                st.run_app_action(crate::shortcuts::AppAction::Take, act);
            }
        });
    });
    monitor_image(ui, cue, cued.is_some() && snap.cue.is_some());
    crate::transport::preview_controls(ui, st, snap, act);
    ui.horizontal(|ui| {
        let hover = snap.preview_audio_device.clone().unwrap_or_else(|| "Preview audio off — Composition → Audio outputs".into());
        ui.label("🎧").on_hover_text(hover);
        if st.audio.preview.is_none() {
            ui.label(RichText::new("Preview audio off — Composition → Audio outputs").small().weak());
        } else {
            if crate::widgets::volume_fader(ui, &mut st.audio.preview_volume) {
                act.commands.push(Command::SetPreviewVolume(st.audio.preview_volume));
                st.audio_dirty = true;
            }
            let now = ui.input(|i| i.time);
            crate::meters::hmeter(ui, snap.preview_peaks, &mut st.preview_holds, now, 120.0);
        }
    });
}


const SLOT: egui::Vec2 = egui::Vec2::new(thumbs::W as f32 * 0.75, thumbs::H as f32 * 0.75);

pub(crate) fn grid(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, thumbs: &mut Thumbnailer, act: &mut Actions) {
    let content = crate::lock::allowed(st.locked, crate::lock::Op::Content);
    if st.map_keys {
        let orange = Color32::from_rgb(255, 150, 40);
        egui::Frame::new().fill(orange).inner_margin(6.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("MAPPING KEYS").strong().color(Color32::BLACK));
                ui.label(RichText::new("click a slot, a scene or a layer ✖ — then press a key · Esc removes it").color(Color32::BLACK));
                if ui.button("Done").clicked() {
                    st.map_keys = false;
                    st.map_target = None;
                    st.status.clear();
                }
            });
        });
        let r = ui.max_rect();
        ui.painter().rect_stroke(r, 0.0, Stroke::new(3.0, orange), StrokeKind::Inside);
    }
    ui.horizontal(|ui| {
        for i in 0..st.project.decks.len() {
            let name = st.project.decks[i].name.clone();
            let r = ui.selectable_label(st.project.active_deck == i, name);
            if r.clicked() {
                st.project.active_deck = i;
            }
            r.context_menu(|ui| {
                if ui.button("Clear ✔ marks").on_hover_text("Forget which clips of this deck have been on air").clicked() {
                    for c in st.project.decks[i].slots.iter_mut().flatten().flatten() {
                        c.aired = false;
                    }
                    st.dirty = true;
                    ui.close();
                }
            });
        }
        if ui.add_enabled(content, egui::Button::new("+ Deck")).clicked() {
            st.project.add_deck();
            st.project.active_deck = st.project.decks.len() - 1;
            st.dirty = true;
        }
    });
    ui.separator();
    st.slot_rects.clear();
    let cols = st.project.columns();
    let layers = st.project.composition.layers.len();
    egui::ScrollArea::both().show(ui, |ui| {
        // Scene headers: 1 click = the whole column on Preview, double-click = Program.
        ui.horizontal(|ui| {
            ui.add_sized(vec2(200.0, 22.0), egui::Label::new("Layer"));
            for c in 0..cols {
                scene_header(ui, st, c, act);
            }
            if ui.add_enabled(content, egui::Button::new("+ Column")).clicked() {
                st.project.add_column();
                st.dirty = true;
            }
        });
        for layer in (0..layers).rev() {
            ui.horizontal(|ui| {
                layer_controls(ui, st, snap, layer, act);
                for col in 0..cols {
                    slot(ui, st, thumbs, layer, col, act);
                }
            });
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(content, egui::Button::new("+ Layer")).clicked() {
                st.project.add_layer();
                st.layers_changed(act);
            }
        });
    });
}

fn scene_header(ui: &mut egui::Ui, st: &mut UiState, c: usize, act: &mut Actions) {
    let (rect, resp) = ui.allocate_exact_size(vec2(SLOT.x, 22.0), Sense::click());
    let a = Action::TriggerColumn(c);
    let name = st.project.deck().map(|d| d.scene_name(c)).unwrap_or_default();
    let layers = st.project.composition.layers.len();
    let filled: Vec<usize> = (0..layers).filter(|&l| st.project.deck().and_then(|d| d.clip(l, c)).is_some()).collect();
    let on_air = !filled.is_empty() && filled.iter().all(|&l| st.is_playing(l, c));
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, Color32::from_gray(if filled.is_empty() { 30 } else { 40 }));
    let text = if filled.is_empty() { Color32::from_gray(110) } else { Color32::from_gray(225) };
    painter.text(rect.left_center() + vec2(6.0, 0.0), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(12.0), text);
    if let Some(k) = st.key_label(&a) {
        painter.text(rect.right_center() - vec2(5.0, 0.0), egui::Align2::RIGHT_CENTER, k, egui::FontId::monospace(11.0), Color32::from_rgb(255, 210, 90));
    }
    let stroke = if st.map_target == Some(a.clone()) {
        Stroke::new(2.0, Color32::from_rgb(255, 150, 40))
    } else if on_air {
        Stroke::new(2.0, Color32::from_rgb(235, 60, 60)) // PROGRAM tally
    } else if st.selected == Selection::Scene(c) {
        Stroke::new(2.0, Color32::from_rgb(60, 200, 90)) // PREVIEW tally
    } else if resp.hovered() {
        Stroke::new(1.0, Color32::from_gray(140))
    } else {
        Stroke::new(1.0, Color32::from_gray(55))
    };
    painter.rect_stroke(rect, 3.0, stroke, StrokeKind::Inside);
    if resp.double_clicked() && !st.map_keys && !filled.is_empty() {
        st.click(a.clone(), act); // straight to PROGRAM
        st.selected = Selection::Scene(c);
    } else if resp.clicked() {
        if st.map_keys {
            st.click(a.clone(), act);
        } else if !filled.is_empty() {
            st.selected = Selection::Scene(c);
            st.multi.clear();
            st.status = format!("{name} on PREVIEW — TAKE (Enter) or double-click to put it live");
        }
    }
    let content = crate::lock::allowed(st.locked, crate::lock::Op::Content);
    let resp = resp.on_hover_text("1 click: Preview · double-click: Program");
    resp.context_menu(|ui| {
        if ui.add_enabled(!filled.is_empty(), egui::Button::new("Put on Program")).clicked() {
            st.click(a.clone(), act);
            ui.close();
        }
        if content && ui.button("Rename…").clicked() {
            st.rename_scene = Some((c, name.clone()));
            ui.close();
        }
    });
}

fn layer_controls(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, layer: usize, act: &mut Actions) {
    let state = snap.layers.get(layer).cloned().unwrap_or_default();
    ui.allocate_ui(vec2(200.0, SLOT.y + 16.0), |ui| {
        ui.vertical(|ui| {
            let clear_key = st.key_label(&Action::ClearLayer(layer)).map(|k| format!(" {k}")).unwrap_or_default();
            let selected = st.selected == Selection::Layer(layer);
            let (mut select, mut clear) = (false, false);
            let l = &mut st.project.composition.layers[layer];
            let before = l.clone();
            ui.horizontal(|ui| {
                select = ui.selectable_label(selected, &l.name).clicked();
                ui.toggle_value(&mut l.bypass, "B").on_hover_text("Bypass");
                ui.toggle_value(&mut l.solo, "S").on_hover_text("Solo");
                clear = ui.button(format!("✖{clear_key}")).on_hover_text("Clear layer").clicked();
            });
            crate::widgets::opacity_bar(ui, &mut l.opacity, layer_color(layer));
            ui.horizontal(|ui| {
                ui.toggle_value(&mut l.mute, "M").on_hover_text("Mute sound");
                crate::widgets::volume_fader(ui, &mut l.volume);
            });
            if *l != before {
                act.commands.push(Command::SetLayer { layer, props: l.clone() });
                st.dirty = true;
            }
            let now = ui.input(|i| i.time);
            if st.layer_holds.len() <= layer {
                st.layer_holds.resize(layer + 1, Default::default());
            }
            crate::meters::hmeter(ui, state.audio_peaks, &mut st.layer_holds[layer], now, 170.0);
            if select {
                st.selected = Selection::Layer(layer);
            }
            if clear {
                st.click(Action::ClearLayer(layer), act);
            }
            let line = match (&state.error, &state.clip_name) {
                (Some(e), _) => egui::RichText::new(e).color(Color32::LIGHT_RED),
                (None, Some(_)) if state.duration > 0.0 => egui::RichText::new(format!("{} / {}", time(state.pos), time(state.duration))),
                (None, _) if state.loading => egui::RichText::new("loading…"),
                _ => egui::RichText::new(""),
            };
            ui.add(egui::Label::new(line.small()).truncate());
        });
    });
}

fn slot(ui: &mut egui::Ui, st: &mut UiState, thumbs: &mut Thumbnailer, layer: usize, col: usize, act: &mut Actions) {
    let (rect, resp) = ui.allocate_exact_size(SLOT + vec2(0.0, 16.0), Sense::click());
    st.slot_rects.push((rect, layer, col));
    let clip = st.project.deck().and_then(|d| d.clip(layer, col)).cloned();
    let painter = ui.painter();
    let img = Rect::from_min_size(rect.min, SLOT);
    painter.rect_filled(rect, 3.0, Color32::from_gray(28));
    if let Some(c) = &clip {
        match thumbs.get(&c.path) {
            Thumb::Ready { tex, .. } => {
                painter.image(tex.id(), img, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            }
            Thumb::Failed(_) => {
                painter.rect_filled(img, 3.0, Color32::from_rgb(90, 20, 20));
            }
            Thumb::Pending => {}
        }
        let text_pos = egui::pos2(rect.min.x + 3.0, rect.max.y - 14.0);
        painter.text(text_pos, egui::Align2::LEFT_TOP, &c.name, egui::FontId::proportional(11.0), Color32::from_gray(220));
    }
    if clip.as_ref().is_some_and(|c| c.audio && thumbs.info(&c.path).is_some_and(|i| i.has_audio)) {
        painter.text(rect.left_top() + vec2(4.0, 3.0), egui::Align2::LEFT_TOP, "♪", egui::FontId::proportional(12.0), Color32::from_rgb(120, 230, 200));
    }
    // CPU-decoded (FFmpeg fallback): works, but costs CPU — Convert to HAP is the cure.
    if clip.as_ref().is_some_and(|c| thumbs.info(&c.path).is_some_and(|i| i.kind == DecoderKind::Ffmpeg)) {
        painter.text(rect.left_top() + vec2(18.0, 3.0), egui::Align2::LEFT_TOP, "HEAVY", egui::FontId::proportional(10.0), Color32::from_rgb(255, 150, 40));
    }
    if let Some(i) = clip.as_ref().and_then(|c| thumbs.info(&c.path)).filter(|i| i.duration > 0.0) {
        let d = crate::transport::clock(i.duration);
        let galley = painter.layout_no_wrap(d, egui::FontId::monospace(10.0), Color32::from_gray(235));
        let at = img.right_bottom() + vec2(-4.0 - galley.size().x, -3.0 - galley.size().y);
        painter.rect_filled(Rect::from_min_size(at - vec2(3.0, 1.0), galley.size() + vec2(6.0, 2.0)), 3.0, Color32::from_black_alpha(170));
        painter.galley(at, galley, Color32::from_gray(235));
    }
    if let Some(c) = &clip {
        if st.importing.contains_key(&c.path) {
            painter.rect_filled(img, 3.0, Color32::from_black_alpha(170));
            painter.text(img.center(), egui::Align2::CENTER_CENTER, "Importing…", egui::FontId::proportional(12.0), Color32::from_rgb(255, 210, 90));
        } else if st.import_errors.contains_key(&c.path) {
            painter.rect_filled(img, 3.0, Color32::from_rgb(90, 20, 20));
            painter.text(img.center(), egui::Align2::CENTER_CENTER, "Import failed", egui::FontId::proportional(12.0), Color32::from_rgb(255, 150, 150));
        } else if evj_core::slides::is_presentation(&c.path) {
            painter.text(img.center(), egui::Align2::CENTER_CENTER, "Not imported", egui::FontId::proportional(12.0), Color32::from_gray(170));
        }
        if c.aired {
            painter.text(img.left_bottom() + vec2(4.0, -3.0), egui::Align2::LEFT_BOTTOM, "✔", egui::FontId::proportional(14.0), Color32::from_rgb(70, 200, 110));
        }
    }
    if clip.is_some() && st.project.panic_media == Some((st.project.active_deck, layer, col)) {
        painter.text(rect.right_top() + vec2(-4.0, 16.0), egui::Align2::RIGHT_TOP, "PANIC", egui::FontId::proportional(10.0), Color32::from_rgb(235, 60, 60));
    }
    if let Some(k) = st.key_label(&Action::TriggerSlot { layer, col }) {
        painter.text(rect.right_top() + vec2(-4.0, 3.0), egui::Align2::RIGHT_TOP, k, egui::FontId::monospace(11.0), Color32::from_rgb(255, 210, 90));
    }
    // Sequence membership: a numbered bar along the bottom of the slot.
    if let Some((_, seq)) = st.project.deck().and_then(|d| d.sequence_at(layer, col)) {
        let n = seq.cols.iter().position(|&c| c == col).unwrap_or(0) + 1;
        let cyan = Color32::from_rgb(80, 200, 255);
        let bar = Rect::from_min_max(egui::pos2(rect.min.x, rect.max.y - 3.0), rect.max);
        painter.rect_filled(bar, 0.0, cyan);
        let tag = format!("{n}/{}{}", seq.cols.len(), if seq.loop_all { " ⟲" } else { "" });
        painter.text(egui::pos2(rect.max.x - 4.0, rect.max.y - 5.0), egui::Align2::RIGHT_BOTTOM, tag, egui::FontId::monospace(10.0), cyan);
    }
    let stroke = if st.map_target == Some(Action::TriggerSlot { layer, col }) {
        Stroke::new(2.0, Color32::from_rgb(255, 150, 40))
    } else if st.drop_target == Some((layer, col)) {
        Stroke::new(2.0, Color32::from_rgb(80, 160, 255))
    } else if st.is_playing(layer, col) {
        Stroke::new(2.0, Color32::from_rgb(235, 60, 60)) // PROGRAM tally
    } else if st.selected == Selection::Slot(layer, col) {
        Stroke::new(2.0, Color32::from_rgb(60, 200, 90)) // PREVIEW tally
    } else if st.multi.contains(&(layer, col)) {
        Stroke::new(2.0, Color32::from_rgb(80, 200, 255)) // shift-click selection
    } else if resp.hovered() {
        Stroke::new(1.0, Color32::from_gray(140))
    } else {
        Stroke::new(1.0, Color32::from_gray(55))
    };
    painter.rect_stroke(rect, 3.0, stroke, StrokeKind::Inside);

    let content = crate::lock::allowed(st.locked, crate::lock::Op::Content);
    if resp.double_clicked() && clip.is_some() && !st.map_keys {
        st.trigger(layer, col, act); // straight to PROGRAM
        st.selected = Selection::Slot(layer, col);
    } else if resp.clicked() {
        if st.map_keys {
            st.click(Action::TriggerSlot { layer, col }, act);
        } else if clip.is_some() && ui.input(|i| i.modifiers.shift) {
            crate::sequence::toggle_multi(st, layer, col);
            st.status = format!("{} slot(s) selected — right-click → Make sequence", st.multi.len());
        } else if clip.is_some() {
            st.multi.clear();
            st.selected = Selection::Slot(layer, col); // cue on PREVIEW
            st.status = "On PREVIEW — TAKE (Enter) or double-click to put it live".into();
        } else if content {
            act.menu = Some(Menu::AddClips(layer, col));
        }
    }
    if let Some(e) = clip.as_ref().and_then(|c| st.import_errors.get(&c.path)) {
        resp.clone().on_hover_text(format!("Import failed: {e}"));
    } else if let Some(Thumb::Failed(e)) = clip.as_ref().and_then(|c| thumbs.cache.get(&c.path)) {
        resp.clone().on_hover_text(e.clone());
    }
    resp.context_menu(|ui| {
        if clip.is_some() {
            if ui.button("Properties").clicked() {
                st.selected = Selection::Slot(layer, col);
                ui.close();
            }
            let retry = clip.as_ref().filter(|c| content && evj_core::slides::is_presentation(&c.path) && !st.importing.contains_key(&c.path));
            if let Some(c) = retry {
                if ui.button("Retry import").clicked() {
                    st.importing.insert(c.path.clone(), (st.project.active_deck, layer, col));
                    act.retry_import = Some(c.path.clone());
                    ui.close();
                }
            }
            if ui.button("Set as panic media").clicked() {
                st.project.panic_media = Some((st.project.active_deck, layer, col));
                st.dirty = true;
                ui.close();
            }
            let picked = st.multi.len() >= 2 && st.multi.contains(&(layer, col));
            if content && picked && ui.button(format!("Make sequence ({} clips)", st.multi.len())).clicked() {
                let cols: Vec<usize> = st.multi.iter().map(|(_, c)| *c).collect();
                if st.project.deck_mut().is_some_and(|d| d.make_sequence(layer, &cols)) {
                    st.status = "Sequence made — trigger its first clip to play them one after another".into();
                    st.dirty = true;
                }
                st.multi.clear();
                ui.close();
            }
            let seq = st.project.deck().and_then(|d| d.sequence_at(layer, col)).map(|(i, s)| (i, s.clone()));
            if let Some((i, mut s)) = seq.filter(|_| content) {
                ui.separator();
                ui.label(RichText::new("Sequence").strong());
                let mut changed = ui.checkbox(&mut s.loop_all, "Loop the whole sequence").changed();
                ui.horizontal(|ui| {
                    ui.label("Still images");
                    changed |= ui.add(egui::DragValue::new(&mut s.still_secs).range(1.0..=600.0).suffix(" s")).changed();
                });
                if changed {
                    if let Some(d) = st.project.deck_mut() {
                        d.sequences[i] = s;
                    }
                    st.dirty = true;
                }
                if ui.button("Break sequence").clicked() {
                    if let Some(d) = st.project.deck_mut() {
                        d.break_sequence(i);
                    }
                    st.dirty = true;
                    ui.close();
                }
                ui.separator();
            }
            if content && ui.button("Remove").clicked() {
                if let Some(d) = st.project.deck_mut() {
                    d.remove(layer, col);
                }
                st.dirty = true;
                ui.close();
            }
        }
        if content && ui.button("Add clips…").clicked() {
            act.menu = Some(Menu::AddClips(layer, col));
            ui.close();
        }
        let convertible = clip.as_ref().filter(|c| content && thumbs.info(&c.path).is_some_and(|i| i.kind != DecoderKind::Hap && i.kind != DecoderKind::Image));
        if let Some(c) = convertible {
            ui.menu_button("Convert to HAP", |ui| {
                for (label, v) in [("HAP (fastest)", HapVariant::Hap), ("HAP Q (best quality)", HapVariant::HapQ), ("HAP Alpha (transparency)", HapVariant::HapAlpha)] {
                    if ui.button(label).clicked() {
                        act.convert = Some((c.path.clone(), v));
                        ui.close();
                    }
                }
            });
        }
    });
}

/// The Properties panel (dock tab).
pub(crate) fn properties_panel(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, thumbs: &Thumbnailer, act: &mut Actions) {
    egui::ScrollArea::vertical().id_salt("properties_panel").show(ui, |ui| properties(ui, st, snap, thumbs, act));
}

fn properties(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, thumbs: &Thumbnailer, act: &mut Actions) {
    ui.horizontal(|ui| {
        if ui.selectable_label(st.selected == Selection::Composition, "Composition").clicked() {
            st.selected = Selection::Composition;
        }
    });
    match st.selected {
        Selection::Slot(layer, col) => clip_props(ui, st, thumbs, snap, layer, col, act),
        Selection::Layer(layer) => layer_props(ui, st, snap, layer, act),
        Selection::Scene(c) => {
            ui.heading(st.project.deck().map(|d| d.scene_name(c)).unwrap_or_default());
            ui.label("Every layer's clip in this column. 1 click: Preview · double-click or TAKE: Program. Layers without a clip here are cleared.");
        }
        Selection::Composition => composition_props(ui, st, snap, act),
        Selection::None => {
            ui.label("Select a clip or layer.");
        }
    }
}

fn clip_props(ui: &mut egui::Ui, st: &mut UiState, thumbs: &Thumbnailer, snap: &Snapshot, layer: usize, col: usize, act: &mut Actions) {
    let content = crate::lock::allowed(st.locked, crate::lock::Op::Content);
    let playing = st.is_playing(layer, col);
    let st_transitions = st.project.transitions.clone();
    let Some(clip) = st.project.deck_mut().and_then(|d| d.clip_mut(layer, col)) else {
        ui.label("Empty slot.");
        return;
    };
    let info = thumbs.info(&clip.path).cloned();
    let before: Clip = clip.clone();
    ui.heading(&clip.name);
    ui.label(egui::RichText::new(clip.path.to_string_lossy()).small());
    let random = info.as_ref().is_none_or(|i| i.random_access);
    if let Some(i) = &info {
        ui.label(format!("{:?} · {}×{} · {:.2} fps · {}", i.kind, i.width, i.height, i.fps, time(i.duration)));
    }
    egui::Grid::new("clip_props").num_columns(2).show(ui, |ui| {
        ui.label("Play");
        egui::ComboBox::from_id_salt("mode").selected_text(format!("{:?}", clip.mode)).show_ui(ui, |ui| {
            ui.selectable_value(&mut clip.mode, PlayMode::Loop, "Loop");
            ui.add_enabled_ui(random, |ui| ui.selectable_value(&mut clip.mode, PlayMode::PingPong, "PingPong"));
            ui.selectable_value(&mut clip.mode, PlayMode::Once, "Once");
        });
        ui.end_row();
        ui.label("Speed");
        let range = if random { -4.0..=4.0 } else { 0.0..=4.0 };
        ui.add(egui::Slider::new(&mut clip.speed, range).step_by(0.05));
        ui.end_row();
        ui.label("BPM sync");
        ui.add_enabled_ui(random, |ui| {
            ui.horizontal(|ui| {
                let mut on = clip.bpm_beats.is_some();
                if ui.checkbox(&mut on, "beats").changed() {
                    clip.bpm_beats = on.then_some(4.0);
                }
                if let Some(b) = clip.bpm_beats.as_mut() {
                    ui.add(egui::DragValue::new(b).range(0.25..=64.0).speed(0.25));
                }
            })
        });
        ui.end_row();
        ui.label("Transition");
        crate::trui::picker(ui, "clip_tr", &mut clip.transition, &st_transitions, "Layer default");
        ui.end_row();
        if info.as_ref().is_some_and(|i| i.has_audio) {
            ui.label("Sound");
            ui.checkbox(&mut clip.audio, "play audio (1x speed only)");
            ui.end_row();
        }
        ui.label("Fit");
        egui::ComboBox::from_id_salt("fit").selected_text(format!("{:?}", clip.fit)).show_ui(ui, |ui| {
            for f in [FitMode::Fit, FitMode::Fill, FitMode::Stretch] {
                ui.selectable_value(&mut clip.fit, f, format!("{f:?}"));
            }
        });
        ui.end_row();
    });
    if !random {
        ui.label(egui::RichText::new("Reverse, ping-pong and BPM sync need random access — convert to HAP. Start / end: see the timeline.").small().weak());
    }
    ui.separator();
    ui.strong("Clip effects");
    ui.add_enabled_ui(content, |ui| crate::fxui::chain(ui, "clipfx", &mut clip.effects, &snap.effects));
    if *clip != before {
        clip.in_point = clip.in_point.min(clip.out_point);
        let c = clip.clone();
        if playing {
            let clip = crate::sequence::as_played(st, layer, col, c);
            act.commands.push(Command::UpdateClip { layer, clip });
        }
        st.dirty = true;
    }
}

fn layer_props(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, layer: usize, act: &mut Actions) {
    let content = crate::lock::allowed(st.locked, crate::lock::Op::Content);
    let n = st.project.composition.layers.len();
    let presets = st.project.transitions.clone();
    let Some(l) = st.project.composition.layers.get_mut(layer) else { return };
    let before = l.clone();
    ui.heading(format!("Layer {}", layer + 1));
    egui::Grid::new("layer_props").num_columns(2).show(ui, |ui| {
        ui.label("Name");
        ui.text_edit_singleline(&mut l.name);
        ui.end_row();
        ui.label("Opacity");
        ui.add(egui::Slider::new(&mut l.opacity, 0.0..=1.0));
        ui.end_row();
        ui.label("Blend");
        egui::ComboBox::from_id_salt("blend").selected_text(format!("{:?}", l.blend)).show_ui(ui, |ui| {
            for m in BlendMode::ALL {
                ui.selectable_value(&mut l.blend, m, format!("{m:?}"));
            }
        });
        ui.end_row();
        ui.label("Transition");
        crate::trui::picker(ui, "layer_tr", &mut l.transition, &presets, "Cut");
        ui.end_row();
        ui.label("");
        ui.horizontal(|ui| {
            ui.checkbox(&mut l.bypass, "Bypass");
            ui.checkbox(&mut l.solo, "Solo");
        });
        ui.end_row();
    });
    ui.separator();
    ui.strong("Layer effects");
    ui.add_enabled_ui(content, |ui| crate::fxui::chain(ui, "layerfx", &mut l.effects, &snap.effects));
    if *l != before {
        act.commands.push(Command::SetLayer { layer, props: l.clone() });
        st.dirty = true;
    }
    if n > 1 && ui.add_enabled(content, egui::Button::new("Delete layer")).clicked() {
        st.project.remove_layer(layer);
        st.playing.remove(layer.min(st.playing.len() - 1));
        st.selected = Selection::None;
        st.layers_changed(act);
        // Clips on the layers above moved down one row: restart them where they now are.
        for i in layer..st.project.composition.layers.len() {
            act.commands.push(Command::Clear { layer: i, transition: None });
            st.playing[i] = None;
        }
    }
}

fn composition_props(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, act: &mut Actions) {
    ui.heading("Composition");
    ui.label(format!("{} layers · {} columns · {} decks", st.project.composition.layers.len(), st.project.columns(), st.project.decks.len()));
    ui.horizontal(|ui| {
        ui.label("Resolution");
        ui.add(egui::DragValue::new(&mut st.res_edit.0).range(16..=16384));
        ui.label("×");
        ui.add(egui::DragValue::new(&mut st.res_edit.1).range(16..=16384));
        let changed = st.res_edit != (st.project.composition.width, st.project.composition.height);
        if ui.add_enabled(changed, egui::Button::new("Apply")).clicked() {
            (st.project.composition.width, st.project.composition.height) = st.res_edit;
            act.commands.push(Command::SetResolution { width: st.res_edit.0, height: st.res_edit.1 });
            act.reopen_preview = true;
            st.dirty = true;
        }
    });
    ui.horizontal(|ui| {
        for (w, h) in [(1920, 1080), (3840, 2160), (1280, 720), (3840, 1080), (1080, 1920)] {
            if ui.small_button(format!("{w}×{h}")).clicked() {
                st.res_edit = (w, h);
            }
        }
    });
    ui.label(format!("Engine: {}×{}", snap.resolution.0, snap.resolution.1));
    ui.separator();
    crate::audioset::panel(ui, st, snap, act);
    ui.separator();
    ui.strong("Composition effects");
    let content = crate::lock::allowed(st.locked, crate::lock::Op::Content);
    if ui.add_enabled_ui(content, |ui| crate::fxui::chain(ui, "compfx", &mut st.project.composition.effects, &snap.effects)).inner {
        act.commands.push(Command::SetCompositionEffects(st.project.composition.effects.clone()));
        st.dirty = true;
    }
    crate::fxui::errors(ui, &snap.effects);
}

/// Stable per-layer colour (tally strips, opacity bars, countdown rows).
pub fn layer_color(i: usize) -> Color32 {
    const C: [Color32; 6] = [
        Color32::from_rgb(40, 150, 230),
        Color32::from_rgb(230, 120, 40),
        Color32::from_rgb(150, 90, 230),
        Color32::from_rgb(40, 190, 170),
        Color32::from_rgb(220, 70, 150),
        Color32::from_rgb(200, 200, 60),
    ];
    C[i % C.len()]
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actions_from_every_egui_pass_are_kept() {
        let mut first = Actions::default();
        first.commands.push(Command::Tap);
        let mut second = Actions::default();
        second.commands.push(Command::CueClip(None));
        second.menu = Some(Menu::Save);
        first.absorb(second);
        assert_eq!(first.commands.len(), 2, "the second pass's command survives");
        assert!(matches!(first.menu, Some(Menu::Save)));
    }

    #[test]
    fn a_new_show_clears_the_preview_cue() {
        let st = UiState::new(Project::new_default());
        assert!(st.sync_engine().iter().any(|c| matches!(c, Command::CueClip(None))));
    }

    #[test]
    fn opening_a_show_keeps_blackout_and_master_level() {
        let mut st = UiState::new(Project::new_default());
        st.blackout = true;
        st.master_volume = 0.0;
        st.replace_project(Project::new_default(), None, &mut Actions::default());
        assert!(st.blackout, "the outputs are still black");
        assert_eq!(st.master_volume, 0.0, "the engine master is still at 0");
    }

    #[test]
    fn a_selected_scene_cues_every_layer_of_its_column() {
        let mut st = UiState::new(Project::new_default());
        st.project.decks[0].slots[0][2] = Some(Clip::new("C:/m/bg.mov".into()));
        st.project.decks[0].slots[2][2] = Some(Clip::new("C:/m/logo.png".into()));
        st.selected = Selection::Scene(2);
        let mut act = Actions::default();
        st.sync_cue(&mut act);
        let scene = act.commands.iter().find_map(|c| match c {
            Command::CueScene(v) => Some(v.clone()),
            _ => None,
        });
        let scene = scene.expect("the scene is cued");
        assert_eq!(scene.len(), st.project.composition.layers.len());
        assert!(scene[0].is_some() && scene[1].is_none() && scene[2].is_some());
        let mut again = Actions::default();
        st.sync_cue(&mut again);
        assert!(again.commands.is_empty(), "sent once");
        let mut act = Actions::default();
        st.run_app_action(crate::shortcuts::AppAction::Take, &mut act);
        let triggers = act.commands.iter().filter(|c| matches!(c, Command::Trigger { .. })).count();
        assert_eq!(triggers, 2, "TAKE puts the scene on Program");
    }

    #[test]
    fn old_toggles_open_their_tab() {
        use crate::dock::{Panel, is_open};
        let mut st = UiState::new(Project::new_default());
        let mut act = Actions::default();
        let pairs = [
            (crate::shortcuts::AppAction::ShowPerf, Panel::Performance),
            (crate::shortcuts::AppAction::ShowOutputs, Panel::Outputs),
            (crate::shortcuts::AppAction::ShowShortcuts, Panel::Shortcuts),
        ];
        for (a, p) in pairs {
            assert!(!is_open(&st.layout, p));
            st.run_app_action(a, &mut act);
            assert!(is_open(&st.layout, p), "{p:?}");
        }
    }

    #[test]
    fn locked_tabs_are_not_closeable() {
        assert!(crate::dock::closeable(false));
        assert!(!crate::dock::closeable(true));
    }

    #[test]
    fn panic_without_media_still_clears() {
        let mut st = UiState::new(Project::new_default()); // empty deck: nothing at layer 1 / column 1
        let mut act = Actions::default();
        st.panic(&mut act);
        let clears = act.commands.iter().filter(|c| matches!(c, Command::Clear { .. })).count();
        assert_eq!(clears, st.project.composition.layers.len());
        assert!(act.commands.iter().any(|c| matches!(c, Command::PanicAudio)));
        assert!(!act.commands.iter().any(|c| matches!(c, Command::Trigger { .. })), "nothing to play");
    }
}
