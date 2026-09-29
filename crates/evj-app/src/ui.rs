//! Control surface: clip grid (Resolume-style), layer controls, preview, properties, file menu.
use crate::outui::OutputPreviews;
use crate::thumbs::{self, Thumb, Thumbnailer};
use evj_media::convert::HapVariant;
use evj_media::DecoderKind;
use evj_core::keymap::Action;
use evj_core::model::{BlendMode, Clip, FitMode, PlayMode, Project, StepMode, StepStart};
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

/// The eyedropper is waiting for a click on a monitor: the clip effect whose key colour it sets.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Eyedrop {
    pub deck: usize,
    pub layer: usize,
    pub col: usize,
    pub effect: usize,
}

/// A delete waiting for confirmation.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Delete {
    Layer(usize),
    Scene(usize),
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
    /// Scene headers and layer clear buttons of the last frame (layout checks, tests).
    pub scene_rects: Vec<(Rect, usize)>,
    pub clear_rects: Vec<(Rect, usize)>,
    pub layer_name_rects: Vec<(Rect, usize)>,
    /// Preview panel of the last frame: (content width, panel width) — layout checks, tests.
    pub preview_width: (f32, f32),
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
    /// Windows default output device (read with `audio_devices`).
    pub default_audio_device: Option<String>,
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
    /// Chains playing, and the UI clock they run on (seconds, set every frame).
    pub layer_runs: Vec<crate::chain::LayerRun>,
    pub scene_run: Option<crate::chain::SceneRun>,
    pub now: f64,
    /// Shift+click multi-selection: slots of one column, and scene headers.
    pub multi: Vec<(usize, usize)>,
    pub multi_scenes: Vec<usize>,
    /// Scene being renamed: (column, name typed so far).
    pub rename_scene: Option<(usize, String)>,
    pub confirm_delete: Option<Delete>,
    /// Eyedropper armed (Pick pressed), and the engine's answer being waited for.
    pub eyedrop: Option<Eyedrop>,
    eyedrop_reply: Option<(Eyedrop, std::sync::mpsc::Receiver<Option<[f32; 3]>>)>,
    /// Program and Preview monitor pictures, and the clip effect editor's widgets, of the last
    /// frame (eyedropper clicks, tests).
    pub monitor_rects: [Option<Rect>; 2],
    pub fx_rects: Vec<(String, Rect)>,
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
            scene_rects: Vec::new(),
            clear_rects: Vec::new(),
            layer_name_rects: Vec::new(),
            preview_width: (0.0, 0.0),
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
            default_audio_device: None,
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
            layer_runs: Vec::new(),
            scene_run: None,
            now: 0.0,
            multi: Vec::new(),
            multi_scenes: Vec::new(),
            rename_scene: None,
            confirm_delete: None,
            eyedrop: None,
            eyedrop_reply: None,
            monitor_rects: [None, None],
            fx_rects: Vec::new(),
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
        self.layer_runs.clear();
        self.scene_run = None;
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
            SeqNext => crate::chain::next_now(self, act),
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

    /// A slot goes to Program; a slot of a layer chain starts the chain there.
    pub fn trigger(&mut self, layer: usize, col: usize, act: &mut Actions) {
        let deck = self.project.active_deck;
        let Some(clip) = self.project.deck().and_then(|d| d.clip(layer, col)).cloned() else { return };
        crate::chain::stop_layer(self, layer);
        if !crate::chain::start_layer_chain(self, deck, col, layer, act) {
            self.play(deck, layer, col, clip, act);
        }
    }

    /// Puts a clip on air as given (the chains call this without stopping themselves).
    pub(crate) fn play(&mut self, deck: usize, layer: usize, col: usize, clip: Clip, act: &mut Actions) {
        let layer_props = self.project.composition.layers.get(layer).cloned().unwrap_or_default();
        let transition = resolve_preset(&self.project.transitions, &clip, self.next_transition.as_deref(), &layer_props).cloned();
        act.commands.push(Command::Trigger { layer, clip, transition });
        if let Some(p) = self.playing.get_mut(layer) {
            *p = Some((deck, col));
        }
        if let Some(c) = self.project.decks.get_mut(deck).and_then(|d| d.clip_mut(layer, col)).filter(|c| !c.aired) {
            c.aired = true;
            self.dirty = true;
        }
    }

    /// A column is a scene: layers without a clip in it are cleared. A scene of a scene chain
    /// starts the chain there.
    pub fn trigger_column(&mut self, col: usize, act: &mut Actions) {
        self.scene_run = None;
        if !crate::chain::start_scene_chain(self, col, act) {
            self.show_scene(self.project.active_deck, col, act);
        }
    }

    /// Column `col` of `deck` on air; a layer chain in it starts at its first step.
    pub(crate) fn show_scene(&mut self, deck: usize, col: usize, act: &mut Actions) {
        for layer in 0..self.project.composition.layers.len() {
            let d = self.project.decks.get(deck);
            let first = d.and_then(|d| d.layer_chain_at(layer, col)).map(|(_, ch)| ch.steps[0].layer);
            let clip = d.and_then(|d| d.clip(layer, col)).cloned();
            match (first, clip) {
                (Some(first), _) if first == layer => {
                    crate::chain::start_layer_chain(self, deck, col, layer, act);
                }
                (Some(_), _) => {} // a later step: its chain cleared it and starts it in time
                (None, Some(clip)) => {
                    crate::chain::stop_layer_runs(self, layer);
                    let clip = crate::chain::as_played_on(self, deck, layer, col, clip);
                    self.play(deck, layer, col, clip, act);
                }
                (None, None) => {
                    crate::chain::stop_layer_runs(self, layer);
                    self.off(layer, act);
                }
            }
        }
    }

    /// Empties a layer (and stops the chains it plays in).
    pub fn clear(&mut self, layer: usize, act: &mut Actions) {
        crate::chain::stop_layer(self, layer);
        self.off(layer, act);
    }

    /// Empties a layer, fading out with the next / layer-default transition.
    pub(crate) fn off(&mut self, layer: usize, act: &mut Actions) {
        let layer_props = self.project.composition.layers.get(layer).cloned().unwrap_or_default();
        let transition = resolve_preset(&self.project.transitions, &Clip::default(), self.next_transition.as_deref(), &layer_props).cloned();
        act.commands.push(Command::Clear { layer, transition });
        if let Some(p) = self.playing.get_mut(layer) {
            *p = None;
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
        self.layer_runs.clear();
        self.scene_run = None;
        act.commands.push(Command::SetLayerCount(n));
        for (i, l) in self.project.composition.layers.iter().enumerate() {
            act.commands.push(Command::SetLayer { layer: i, props: l.clone() });
        }
        self.dirty = true;
    }

    /// ⏶ / ⏷: swaps a layer with the one above / below; what plays on them keeps playing.
    pub fn move_layer(&mut self, layer: usize, up: bool, act: &mut Actions) {
        let n = self.project.composition.layers.len();
        let to = if up { layer.checked_sub(1) } else { Some(layer + 1).filter(|&t| t < n) };
        let Some(to) = to.filter(|_| layer < n) else { return };
        self.project.move_layer(layer, to);
        act.commands.push(Command::MoveLayer { from: layer, to });
        fn swap<T>(v: &mut [T], a: usize, b: usize) {
            if a < v.len() && b < v.len() {
                v.swap(a, b);
            }
        }
        swap(&mut self.playing, layer, to);
        swap(&mut self.layer_holds, layer, to);
        let f = |l: usize| if l == layer { to } else if l == to { layer } else { l };
        self.selected = match self.selected {
            Selection::Slot(l, c) => Selection::Slot(f(l), c),
            Selection::Layer(l) => Selection::Layer(f(l)),
            s => s,
        };
        self.multi.clear();
        self.dirty = true;
    }

    /// Deletes a layer and its clips; the other layers keep playing (only their numbers change).
    pub fn delete_layer(&mut self, layer: usize, act: &mut Actions) {
        let n = self.project.composition.layers.len();
        if n <= 1 || layer >= n {
            return;
        }
        act.commands.push(Command::Clear { layer, transition: None });
        // Walk the deleted layer to the end; SetLayerCount then drops it.
        for i in layer..n - 1 {
            act.commands.push(Command::MoveLayer { from: i, to: i + 1 });
        }
        self.project.remove_layer(layer);
        if layer < self.playing.len() {
            self.playing.remove(layer);
        }
        if layer < self.layer_holds.len() {
            self.layer_holds.remove(layer);
        }
        self.selected = match self.selected {
            Selection::Slot(l, _) | Selection::Layer(l) if l == layer => Selection::None,
            Selection::Slot(l, c) if l > layer => Selection::Slot(l - 1, c),
            Selection::Layer(l) if l > layer => Selection::Layer(l - 1),
            s => s,
        };
        self.multi.clear();
        self.layers_changed(act);
    }

    /// Deletes a scene (column) in every deck; its clips on air are cleared first.
    pub fn delete_scene(&mut self, col: usize, act: &mut Actions) {
        if self.project.columns() <= 1 || col >= self.project.columns() {
            return;
        }
        for l in 0..self.playing.len() {
            match self.playing[l] {
                Some((_, c)) if c == col => self.clear(l, act),
                Some((d, c)) if c > col => self.playing[l] = Some((d, c - 1)),
                _ => {}
            }
        }
        self.project.remove_column(col);
        let shift = |c: usize| (c != col).then(|| if c > col { c - 1 } else { c });
        self.selected = match self.selected {
            Selection::Slot(l, c) => shift(c).map_or(Selection::None, |c| Selection::Slot(l, c)),
            Selection::Scene(c) => shift(c).map_or(Selection::None, Selection::Scene),
            s => s,
        };
        self.multi.clear();
        self.rename_scene = None;
        self.dirty = true;
    }

    /// A question dialog waits for an answer.
    pub fn asking(&self) -> bool {
        self.confirm_delete.is_some() || self.confirm_quit || self.confirm_discard.is_some() || self.recovery.is_some() || self.rename_scene.is_some()
    }

    pub(crate) fn is_playing(&self, layer: usize, col: usize) -> bool {
        self.playing.get(layer).copied().flatten() == Some((self.project.active_deck, col))
    }
}

/// Seconds shown on a slot: an image's own duration, else the file's length once it is read.
pub(crate) fn slot_secs(clip: &Clip, info: Option<&evj_media::ClipInfo>) -> Option<f64> {
    if evj_media::image::is_image(&clip.path) && clip.attached.is_none() {
        return Some(clip.image_secs());
    }
    info.map(|i| i.duration).filter(|d| *d > 0.0)
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
    let asking = st.asking();
    if st.shortcut_armed.is_none() && !st.map_keys {
        for a in crate::shortcuts::resolve(&st.shortcuts, &events, typing) {
            // A question on screen owns the keyboard; PANIC and BLACKOUT always work.
            if asking && !matches!(a, crate::shortcuts::AppAction::Panic | crate::shortcuts::AppAction::Blackout) {
                continue;
            }
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
    let now = ui.input(|i| i.time);
    crate::chain::update(st, snap, now, &mut act);
    eyedrop_answer(st, &mut act);
    if st.eyedrop.is_some() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.eyedrop = None;
        st.status = "Eyedropper cancelled".into();
    }
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
    if ctx.egui_wants_keyboard_input() || st.shortcut_armed.is_some() || st.asking() {
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
                st.status = format!("{} › {target:?}", key.name());
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
    use crate::modal::{Tone, ask, note, text};
    if let Some(d) = st.confirm_delete {
        let (title, what, button) = match d {
            Delete::Layer(l) => ("Delete layer?", st.project.composition.layers.get(l).map(|x| x.name.clone()).unwrap_or_default(), "Delete layer"),
            Delete::Scene(c) => ("Delete scene?", st.project.deck().map(|x| x.scene_name(c)).unwrap_or_default(), "Delete scene"),
        };
        let answer = ask(ctx, "confirm_delete", title, Some(0), &[("Cancel", Tone::Plain), (button, Tone::Danger)], |ui| {
            text(ui, format!("\u{201c}{what}\u{201d} and its clips will be removed from every deck."));
            note(ui, "Anything of it on Program is cleared first. This cannot be undone.");
        });
        if let Some(i) = answer {
            st.confirm_delete = None;
            // A lock since the question was asked cancels it.
            if i == 1 && crate::lock::allowed(st.locked, crate::lock::Op::Content) {
                match d {
                    Delete::Layer(l) => st.delete_layer(l, act),
                    Delete::Scene(c) => st.delete_scene(c, act),
                }
            }
        }
    }
    if st.confirm_quit {
        let buttons: &[(&str, Tone)] = if st.dirty {
            &[("Cancel", Tone::Plain), ("Quit without saving", Tone::Plain), ("Save and quit", Tone::Primary)]
        } else {
            &[("Cancel", Tone::Plain), ("Quit", Tone::Danger)]
        };
        let (output_open, dirty) = (st.output_open, st.dirty);
        let answer = ask(ctx, "confirm_quit", "Quit EVJ?", Some(0), buttons, |ui| {
            if output_open {
                text(ui, "The audience screens will go black.");
            }
            if dirty {
                text(ui, "The show has unsaved changes.");
            }
        });
        match answer {
            Some(0) => st.confirm_quit = false,
            Some(1) => act.quit = true, // Quit / Quit without saving
            Some(2) => {
                act.save_and_quit = true;
                st.confirm_quit = false;
            }
            _ => {}
        }
    }
    if st.confirm_discard.is_some() {
        let answer = ask(ctx, "confirm_discard", "Save changes first?", Some(0), &[("Cancel", Tone::Plain), ("Don't save", Tone::Plain), ("Save", Tone::Primary)], |ui| {
            text(ui, "The current show has changes that are not saved yet.");
        });
        match answer {
            Some(0) => st.confirm_discard = None,
            Some(1) => act.discard_answer = Some(false),
            Some(2) => act.discard_answer = Some(true),
            _ => {}
        }
    }
    if st.recovery.is_some() {
        let answer = ask(ctx, "recover", "Recover the show?", None, &[("Discard", Tone::Plain), ("Recover", Tone::Primary)], |ui| {
            text(ui, "EVJ did not close normally last time.");
            note(ui, "Recover opens the show as it was autosaved.");
        });
        match answer {
            Some(0) => act.menu = Some(Menu::Recover(false)),
            Some(1) => act.menu = Some(Menu::Recover(true)),
            _ => {}
        }
    }
    if let Some((c, mut name)) = st.rename_scene.clone() {
        let answer = ask(ctx, "rename_scene", "Rename scene", Some(0), &[("Cancel", Tone::Plain), ("Rename", Tone::Primary)], |ui| {
            let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY).hint_text("Scene name (empty = Scene N)"));
            r.request_focus();
        });
        st.rename_scene = match answer {
            Some(1) => {
                if let Some(d) = st.project.deck_mut() {
                    d.set_scene_name(c, &name);
                }
                st.dirty = true;
                None
            }
            Some(_) => None,
            None => Some((c, name)),
        };
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
/// `max_h`: the picture never takes more than this height (the rows under it stay visible);
/// it is centred when the height, not the width, limits it.
/// While the eyedropper is armed, a click on a monitor asks the engine for the clip's colour there.
fn eyedrop_click(ui: &mut egui::Ui, st: &mut UiState, rect: Rect, program: bool, act: &mut Actions) {
    let Some(e) = st.eyedrop else { return };
    let resp = ui.interact(rect, ui.id().with(("eyedrop", program)), Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    let Some(p) = resp.clicked().then(|| resp.interact_pointer_pos()).flatten() else { return };
    let pos = [((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0), ((p.y - rect.top()) / rect.height()).clamp(0.0, 1.0)];
    let here = e.deck == st.project.active_deck;
    let at = if program {
        if !(here && st.is_playing(e.layer, e.col)) {
            st.status = "This clip is not on Program — click its colour on the Preview".into();
            return;
        }
        evj_engine::PickAt::Program(e.layer)
    } else {
        if !(here && st.selected == Selection::Slot(e.layer, e.col)) {
            st.status = "The Preview shows another clip — select this clip again".into();
            return;
        }
        evj_engine::PickAt::Preview
    };
    let (tx, rx) = std::sync::mpsc::channel();
    act.commands.push(Command::PickColor { at, pos, reply: tx });
    st.eyedrop = None;
    st.eyedrop_reply = Some((e, rx));
}

/// The engine's answer to an eyedropper click: the colour becomes the effect's key colour.
fn eyedrop_answer(st: &mut UiState, act: &mut Actions) {
    use std::sync::mpsc::TryRecvError;
    let Some((e, rx)) = st.eyedrop_reply.take() else { return };
    let rgb = match rx.try_recv() {
        Ok(Some(rgb)) => rgb,
        Ok(None) => {
            st.status = "That click was beside the clip — press Pick and try again".into();
            return;
        }
        Err(TryRecvError::Empty) => {
            st.eyedrop_reply = Some((e, rx));
            return;
        }
        Err(TryRecvError::Disconnected) => return,
    };
    if !crate::lock::allowed(st.locked, crate::lock::Op::Content) {
        return;
    }
    let Some(clip) = st.project.decks.get_mut(e.deck).and_then(|d| d.clip_mut(e.layer, e.col)) else { return };
    let Some(fx) = clip.effects.get_mut(e.effect) else { return };
    for (k, v) in ["key_r", "key_g", "key_b"].iter().zip(rgb) {
        fx.param_mut(k, 0.0).value = v as f64;
    }
    let clip = clip.clone();
    st.dirty = true;
    st.status = format!("Key colour picked: R {:.0} G {:.0} B {:.0}", rgb[0] * 255.0, rgb[1] * 255.0, rgb[2] * 255.0);
    if e.deck == st.project.active_deck && st.is_playing(e.layer, e.col) {
        let clip = crate::chain::as_played(st, e.layer, e.col, clip);
        act.commands.push(Command::UpdateClip { layer: e.layer, clip });
    }
}

fn monitor_image(ui: &mut egui::Ui, tex: Option<(egui::TextureId, [f32; 2])>, show: bool, max_h: f32) -> Rect {
    let w = ui.available_width();
    let aspect = tex.map_or(16.0 / 9.0, |(_, s)| s[0] / s[1].max(1.0));
    let h = (w / aspect).min(max_h.max(60.0));
    let size = vec2(h * aspect, h);
    let pad = (w - size.x).max(0.0) / 2.0;
    ui.horizontal(|ui| {
        ui.add_space(pad);
        let (r, _) = ui.allocate_exact_size(size, Sense::hover());
        match tex.filter(|_| show) {
            Some((id, _)) => {
                ui.painter().image(id, r, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                ui.painter().rect_filled(r, 2.0, Color32::BLACK);
            }
        }
        r
    })
    .inner
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
    // Room under the picture for every layer on air (one row each).
    let rows = snap.layers.iter().filter(|l| crate::transport::on_air(l)).count().max(1) as f32;
    let r = monitor_image(ui, program, true, ui.available_height() - 12.0 - rows * 26.0);
    st.monitor_rects[0] = Some(r);
    eyedrop_click(ui, st, r, true, act);
    if snap.blackout > 0.0 {
        ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha((snap.blackout * 255.0) as u8));
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, "BLACKOUT", egui::FontId::proportional(18.0), Color32::from_rgb(235, 60, 60));
    }
    crate::transport::program_strip(ui, st, snap, act);
}

/// The Preview panel: the cued clip / scene, TAKE, its transport and its sound.
/// The Preview panel: the cued clip / scene, TAKE, transport, its sound, its timeline (seek,
/// start / end, loop, attached audio) and its settings (effects …).
pub(crate) fn preview_panel(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, cue: Option<(egui::TextureId, [f32; 2])>, thumbs: &mut Thumbnailer, waves: &mut crate::waveform::Waveforms, act: &mut Actions) {
    let avail = ui.available_width();
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
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let take_key = st.shortcuts.combo(crate::shortcuts::AppAction::Take).map(|c| format!(" ({c})")).unwrap_or_default();
            let take = ui.add_enabled(cued.is_some(), egui::Button::new(RichText::new("TAKE ▶").strong())).on_hover_text(format!("Put the Preview on Program (with the next / layer transition){take_key}"));
            if take.clicked() {
                st.run_app_action(crate::shortcuts::AppAction::Take, act);
            }
            // The name gets what is left (long names end in "…"; hover shows it whole).
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                match &cued {
                    Some(text) => ui.add(egui::Label::new(RichText::new(text).small()).truncate()).on_hover_text(text),
                    None => ui.add(egui::Label::new(RichText::new("click a clip or a scene to cue it").small().weak()).truncate()),
                };
            });
        });
    });
    // At most about half the panel: transport, timeline and clip settings follow.
    let r = monitor_image(ui, cue, cued.is_some() && snap.cue.is_some(), (ui.available_height() * 0.5).max(120.0));
    st.monitor_rects[1] = Some(r);
    eyedrop_click(ui, st, r, false, act);
    crate::transport::preview_controls(ui, st, snap, act);
    ui.horizontal(|ui| {
        let hover = snap.preview_audio_device.clone().unwrap_or_else(|| "Preview audio off — Composition › Audio outputs".into());
        ui.label("🎧").on_hover_text(hover);
        if st.audio.preview.is_none() {
            ui.label(RichText::new("Preview audio off — Composition › Audio outputs").small().weak());
        } else {
            if crate::widgets::volume_fader(ui, &mut st.audio.preview_volume) {
                act.commands.push(Command::SetPreviewVolume(st.audio.preview_volume));
                st.audio_dirty = true;
            }
            let now = ui.input(|i| i.time);
            crate::meters::hmeter(ui, snap.preview_peaks, &mut st.preview_holds, now, 120.0);
        }
    });
    ui.separator();
    crate::timeline::panel(ui, st, snap, waves, thumbs, act);
    if let Selection::Slot(l, c) = st.selected {
        ui.separator();
        egui::CollapsingHeader::new(RichText::new("Clip settings & effects").strong()).id_salt("clip_settings").default_open(true).show(ui, |ui| {
            clip_props(ui, st, thumbs, snap, l, c, act);
        });
    }
    chain_settings(ui, st);
    st.preview_width = (ui.min_rect().width(), avail);
}


/// Preview panel: the chains of the selected slot / scene.
fn chain_settings(ui: &mut egui::Ui, st: &mut UiState) {
    let deck = st.project.active_deck;
    let (slot, col) = match st.selected {
        Selection::Slot(l, c) => (Some(l), c),
        Selection::Scene(c) => (None, c),
        _ => return,
    };
    let Some(d) = st.project.deck() else { return };
    let layer_chain = slot.and_then(|l| d.layer_chain_at(l, col)).map(|(i, ch)| (i, ch.clone()));
    let scene_chain = d.scene_chain_at(col).map(|(i, ch)| (i, ch.clone()));
    if layer_chain.is_none() && scene_chain.is_none() {
        return;
    }
    let content = crate::lock::allowed(st.locked, crate::lock::Op::Content);
    ui.separator();
    egui::CollapsingHeader::new(RichText::new("Chain").strong()).id_salt("chain_settings").default_open(true).show(ui, |ui| {
        ui.add_enabled_ui(content, |ui| {
            if let Some((i, mut ch)) = layer_chain {
                ui.label(RichText::new("Layer chain — these layers of the scene start one after another").color(chain_color(i)));
                let before = ch.clone();
                // One wrapping row per step: fits a narrow Preview tab.
                for (k, step) in ch.steps.iter_mut().enumerate() {
                    let name = st.project.composition.layers.get(step.layer).map(|l| l.name.clone()).unwrap_or_default();
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("{}. {name}", k + 1)).color(layer_color(step.layer)));
                        if k == 0 {
                            ui.label(RichText::new("starts the chain").weak());
                            return;
                        }
                        let timed = matches!(step.start, StepStart::AfterSecs(_));
                        if ui.selectable_label(!timed, "when the one before ends").clicked() {
                            step.start = StepStart::AfterPrevious;
                        }
                        if ui.selectable_label(timed, "after").on_hover_text("N seconds after the one before started").clicked() && !timed {
                            step.start = StepStart::AfterSecs(10.0);
                        }
                        if let StepStart::AfterSecs(n) = &mut step.start {
                            ui.add(egui::DragValue::new(n).range(0.0..=3600.0).speed(0.1).suffix(" s"));
                        }
                        ui.label("·");
                        ui.selectable_value(&mut step.mode, StepMode::Replace, "Replace").on_hover_text("Clear the one before");
                        ui.selectable_value(&mut step.mode, StepMode::Overlay, "Overlay 🗗").on_hover_text("Keep the one before playing");
                    });
                }
                ui.horizontal_wrapped(|ui| {
                    ui.radio_value(&mut ch.looping, false, "■ Stop at the end");
                    ui.radio_value(&mut ch.looping, true, "⟲ Loop (clear, start again)");
                });
                if ch != before {
                    crate::chain::edit_layer_chain(st, deck, i, ch);
                }
                if ui.button("Break layer chain").clicked() {
                    st.project.decks[deck].break_layer_chain(i);
                    st.dirty = true;
                }
            }
            if let Some((i, mut ch)) = scene_chain {
                ui.add_space(4.0);
                let list = ch.cols.iter().map(|c| format!("{}", c + 1)).collect::<Vec<_>>().join(" › ");
                ui.label(RichText::new(format!("Scene chain: scenes {list}")).color(chain_color(i)));
                ui.label(RichText::new("The next scene starts when the longest clip ends; A–B loops and looping layer chains wait for ⏭ Next.").small().weak());
                let before = ch.clone();
                ui.horizontal_wrapped(|ui| {
                    ui.radio_value(&mut ch.looping, false, "■ Stop at the end");
                    ui.radio_value(&mut ch.looping, true, "⟲ Loop");
                });
                if ch != before {
                    crate::chain::edit_scene_chain(st, deck, i, ch);
                }
                if ui.button("Break scene chain").clicked() {
                    st.project.decks[deck].break_scene_chain(i);
                    st.dirty = true;
                }
            }
        });
    });
}

const SLOT: egui::Vec2 = egui::Vec2::new(thumbs::W as f32 * 0.75, thumbs::H as f32 * 0.75);
/// Width of the layer controls column; the scene header row starts with the same width so the
/// headers sit exactly above their slots.
const LAYER_COL: f32 = 200.0;

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
    st.scene_rects.clear();
    st.clear_rects.clear();
    st.layer_name_rects.clear();
    let cols = st.project.columns();
    let layers = st.project.composition.layers.len();
    // The layer column stays put; only the slots (and their scene headers) scroll sideways.
    // Scrolling down moves both, so every layer row stays beside its slots.
    egui::ScrollArea::vertical().id_salt("grid_rows").show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.add_sized(vec2(LAYER_COL, 22.0), egui::Label::new("Layer"));
                for layer in 0..layers {
                    layer_controls(ui, st, snap, layer, act);
                }
                if ui.add_enabled(content, egui::Button::new("+ Layer")).clicked() {
                    st.project.add_layer();
                    st.layers_changed(act);
                }
            });
            egui::ScrollArea::horizontal().id_salt("grid_slots").show(ui, |ui| {
                ui.vertical(|ui| {
                    // Scene headers: 1 click = the whole column on Preview, double-click = Program.
                    ui.horizontal(|ui| {
                        for c in 0..cols {
                            scene_header(ui, st, c, act);
                        }
                        if ui.add_enabled(content, egui::Button::new("+ Column")).clicked() {
                            st.project.add_column();
                            st.dirty = true;
                        }
                    });
                    for layer in 0..layers {
                        ui.horizontal(|ui| {
                            for col in 0..cols {
                                slot(ui, st, thumbs, layer, col, act);
                            }
                        });
                    }
                });
            });
        });
    });
}

/// A keyboard shortcut on a slot / scene: black on yellow, easy to read on any thumbnail.
fn key_badge(painter: &egui::Painter, top_right: egui::Pos2, key: &str) {
    let galley = painter.layout_no_wrap(key.to_string(), egui::FontId::monospace(13.0), Color32::BLACK);
    let size = galley.size() + vec2(8.0, 2.0);
    let r = Rect::from_min_size(top_right - vec2(size.x, 0.0), size);
    painter.rect_filled(r, 3.0, Color32::from_rgb(255, 205, 40));
    painter.rect_stroke(r, 3.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Inside);
    painter.galley(r.min + vec2(4.0, 1.0), galley, Color32::BLACK);
}

fn scene_header(ui: &mut egui::Ui, st: &mut UiState, c: usize, act: &mut Actions) {
    let (rect, resp) = ui.allocate_exact_size(vec2(SLOT.x, 22.0), Sense::click());
    st.scene_rects.push((rect, c));
    let a = Action::TriggerColumn(c);
    let name = st.project.deck().map(|d| d.scene_name(c)).unwrap_or_default();
    let layers = st.project.composition.layers.len();
    let filled: Vec<usize> = (0..layers).filter(|&l| st.project.deck().and_then(|d| d.clip(l, c)).is_some()).collect();
    let on_air = !filled.is_empty() && filled.iter().all(|&l| st.is_playing(l, c));
    let chain = st.project.deck().and_then(|d| d.scene_chain_at(c)).map(|(i, ch)| (i, ch.clone()));
    let painter = ui.painter();
    painter.rect_filled(rect, 3.0, Color32::from_gray(if filled.is_empty() { 30 } else { 40 }));
    let text = if filled.is_empty() { Color32::from_gray(110) } else { Color32::from_gray(225) };
    // Scene chain: a band along the bottom, the step number before the name.
    let mut x = rect.left() + 6.0;
    if let Some((i, ch)) = &chain {
        let color = chain_color(*i);
        let k = ch.cols.iter().position(|&x| x == c).unwrap_or(0);
        let tag = if k == 0 { format!("1{}", if ch.looping { "⟲" } else { "■" }) } else { format!("{}", k + 1) };
        let r = painter.text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, tag, egui::FontId::monospace(11.0), color);
        x = r.right() + 5.0;
        painter.rect_filled(Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 3.0), rect.right_bottom()), 0.0, color);
    }
    painter.text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(12.0), text);
    if let Some(k) = st.key_label(&a) {
        key_badge(painter, rect.right_top() + vec2(-3.0, 2.0), &k);
    }
    let stroke = if st.map_target == Some(a.clone()) {
        Stroke::new(2.0, Color32::from_rgb(255, 150, 40))
    } else if on_air {
        Stroke::new(2.0, Color32::from_rgb(235, 60, 60)) // PROGRAM tally
    } else if st.selected == Selection::Scene(c) {
        Stroke::new(2.0, Color32::from_rgb(60, 200, 90)) // PREVIEW tally
    } else if st.multi_scenes.contains(&c) {
        Stroke::new(2.0, Color32::from_rgb(80, 200, 255)) // shift-click selection
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
        } else if !filled.is_empty() && ui.input(|i| i.modifiers.shift) {
            crate::chain::toggle_scene(st, c);
            st.status = format!("{} scene(s) selected — right-click › Chain scenes", st.multi_scenes.len());
        } else if !filled.is_empty() {
            st.selected = Selection::Scene(c);
            st.multi.clear();
            st.multi_scenes.clear();
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
        let picked = st.multi_scenes.len() >= 2 && st.multi_scenes.contains(&c);
        let label = if picked { format!("Chain scenes ({})", st.multi_scenes.len()) } else { "Chain scenes…".to_string() };
        let chain_b = ui.add_enabled(content && picked, egui::Button::new(label));
        if !picked {
            ui.label(RichText::new("Shift+click 2 or more scene headers (or slots of different scenes) first").small().weak());
        }
        if chain_b.clicked() {
            let cols = std::mem::take(&mut st.multi_scenes);
            if st.project.deck_mut().is_some_and(|d| d.make_scene_chain(&cols)) {
                st.status = "Scenes chained — play the first one; each starts when the one before ends".into();
                st.dirty = true;
            }
            ui.close();
        }
        if let Some((i, _)) = chain.as_ref().filter(|_| content) {
            if ui.button("Break scene chain").clicked() {
                if let Some(d) = st.project.deck_mut() {
                    d.break_scene_chain(*i);
                }
                st.dirty = true;
                ui.close();
            }
        }
        if content && st.project.columns() > 1 && ui.button("Delete scene…").clicked() {
            st.confirm_delete = Some(Delete::Scene(c));
            ui.close();
        }
    });
}

/// A layer's right-click menu (its name, or anywhere on its controls).
fn layer_menu(ui: &mut egui::Ui, content: bool, layer: usize, n: usize, moved: &mut Option<bool>, delete: &mut bool) {
    if ui.add_enabled(content && layer > 0, egui::Button::new("Move up")).clicked() {
        *moved = Some(true);
        ui.close();
    }
    if ui.add_enabled(content && layer + 1 < n, egui::Button::new("Move down")).clicked() {
        *moved = Some(false);
        ui.close();
    }
    ui.separator();
    let del = ui.add_enabled(content && n > 1, egui::Button::new("Delete layer…"));
    let del = if n <= 1 { del.on_disabled_hover_text("The last layer stays") } else { del.on_disabled_hover_text("Locked: unlock to delete") };
    if del.clicked() {
        *delete = true;
        ui.close();
    }
}

fn layer_controls(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, layer: usize, act: &mut Actions) {
    let state = snap.layers.get(layer).cloned().unwrap_or_default();
    ui.allocate_ui(vec2(LAYER_COL, SLOT.y + 16.0), |ui| {
        // Under the controls (registered first, so they stay on top): right-click opens the layer menu.
        let block = Rect::from_min_size(ui.max_rect().min, vec2(LAYER_COL, SLOT.y + 16.0));
        let block = ui.interact(block, ui.id().with(("layer_block", layer)), Sense::click());
        ui.vertical(|ui| {
            ui.set_width(LAYER_COL); // exactly the header's width, whatever the controls need
            ui.set_height(SLOT.y + 16.0); // exactly a slot row, so the rows beside it line up
            let clear_key = st.key_label(&Action::ClearLayer(layer)).map(|k| format!(" {k}")).unwrap_or_default();
            let selected = st.selected == Selection::Layer(layer);
            let content = crate::lock::allowed(st.locked, crate::lock::Op::Content);
            let n = st.project.composition.layers.len();
            let (mut select, mut clear, mut delete) = (false, false, false);
            let mut moved: Option<bool> = None;
            let mut name_rect = None;
            let mut clear_rect = None;
            let l = &mut st.project.composition.layers[layer];
            let before = l.clone();
            ui.horizontal(|ui| {
                let name = ui.selectable_label(selected, &l.name).on_hover_text("Right-click: move / delete layer");
                name_rect = Some(name.rect);
                select = name.clicked();
                name.context_menu(|ui| layer_menu(ui, content, layer, n, &mut moved, &mut delete));
                if ui.add_enabled(content && layer > 0, egui::Button::new("⏶").small()).on_hover_text("Move up (drawn above)").clicked() {
                    moved = Some(true);
                }
                if ui.add_enabled(content && layer + 1 < n, egui::Button::new("⏷").small()).on_hover_text("Move down (drawn below)").clicked() {
                    moved = Some(false);
                }
                ui.toggle_value(&mut l.bypass, "B").on_hover_text("Bypass");
                ui.toggle_value(&mut l.solo, "S").on_hover_text("Solo");
                let b = ui.button(format!("✖{clear_key}")).on_hover_text("Clear layer");
                clear_rect = Some(b.rect);
                clear = b.clicked();
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
            if let Some(r) = clear_rect {
                st.clear_rects.push((r, layer));
            }
            if let Some(r) = name_rect {
                st.layer_name_rects.push((r, layer));
            }
            if clear {
                st.click(Action::ClearLayer(layer), act);
            }
            block.context_menu(|ui| layer_menu(ui, content, layer, n, &mut moved, &mut delete));
            if delete {
                st.confirm_delete = Some(Delete::Layer(layer));
            }
            if let Some(up) = moved {
                st.move_layer(layer, up, act);
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
        // Under the picture: ♪ when the clip plays its own sound, then its name.
        let sound = c.audio && thumbs.info(&c.path).is_some_and(|i| i.has_audio);
        let mut x = rect.min.x + 3.0;
        if sound {
            painter.text(egui::pos2(x, rect.max.y - 15.0), egui::Align2::LEFT_TOP, "♪", egui::FontId::proportional(12.0), Color32::from_rgb(120, 230, 200));
            x += 13.0;
        }
        painter.text(egui::pos2(x, rect.max.y - 14.0), egui::Align2::LEFT_TOP, &c.name, egui::FontId::proportional(11.0), Color32::from_gray(220));
    }
    // CPU-decoded (FFmpeg fallback): works, but costs CPU — Convert to HAP is the cure.
    if clip.as_ref().is_some_and(|c| thumbs.info(&c.path).is_some_and(|i| i.kind == DecoderKind::Ffmpeg)) {
        painter.text(rect.left_top() + vec2(18.0, 3.0), egui::Align2::LEFT_TOP, "HEAVY", egui::FontId::proportional(10.0), Color32::from_rgb(255, 150, 40));
    }
    if let Some(secs) = clip.as_ref().and_then(|c| slot_secs(c, thumbs.info(&c.path))) {
        let d = crate::transport::clock(secs);
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
        key_badge(painter, rect.right_top() + vec2(-4.0, 4.0), &k);
    }
    // Layer chain: a line down the left edge joining its slots, the step number on each.
    let spans = |ch: &&evj_core::model::LayerChain| ch.col == col && ch.steps[0].layer <= layer && layer <= ch.steps[ch.steps.len() - 1].layer;
    if let Some((i, ch)) = st.project.deck().and_then(|d| d.layer_chains.iter().enumerate().find(|(_, ch)| spans(ch))) {
        let color = chain_color(i);
        let (first, last) = (ch.steps[0].layer == layer, ch.steps[ch.steps.len() - 1].layer == layer);
        let top = if first { rect.top() + 4.0 } else { rect.top() - ui.spacing().item_spacing.y };
        let bottom = if last { rect.bottom() - 4.0 } else { rect.bottom() };
        painter.rect_filled(Rect::from_min_max(egui::pos2(rect.left(), top), egui::pos2(rect.left() + 4.0, bottom)), 0.0, color);
        if let Some(k) = ch.steps.iter().position(|s| s.layer == layer) {
            let step = ch.steps[k];
            let mut tag = format!("{}", k + 1);
            if k == 0 {
                tag += if ch.looping { " ⟲" } else { " ■" };
            } else {
                if let StepStart::AfterSecs(n) = step.start {
                    tag += &format!(" +{n:.1}s");
                }
                if step.mode == StepMode::Overlay {
                    tag += " 🗗";
                }
            }
            let galley = painter.layout_no_wrap(tag, egui::FontId::monospace(11.0), color);
            let at = egui::pos2(img.left() + 7.0, img.center().y - galley.size().y / 2.0);
            painter.rect_filled(Rect::from_min_size(at - vec2(2.0, 1.0), galley.size() + vec2(4.0, 2.0)), 3.0, Color32::from_black_alpha(190));
            painter.galley(at, galley, color);
        }
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
            crate::chain::toggle_multi(st, layer, col);
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
            // Shift+clicked slots: one scene = a layer chain, several scenes = a scene chain.
            let picked = st.multi.contains(&(layer, col));
            let label = match crate::chain::pick(st).filter(|_| picked) {
                Some(crate::chain::Pick::Layers(_, l)) => Some(format!("Chain layers ({} clips)", l.len())),
                Some(crate::chain::Pick::Scenes(c)) => Some(format!("Chain scenes ({})", c.len())),
                None => None,
            };
            match label {
                Some(label) => {
                    if ui.add_enabled(content, egui::Button::new(label)).clicked() {
                        crate::chain::apply_pick(st);
                        ui.close();
                    }
                }
                None => {
                    ui.add_enabled(false, egui::Button::new("Chain…"));
                    ui.label(RichText::new("Shift+click 2 or more slots first: one scene = layer chain, several scenes = scene chain").small().weak());
                }
            }
            let chain = st.project.deck().and_then(|d| d.layer_chain_at(layer, col)).map(|(i, _)| i);
            if let Some(i) = chain.filter(|_| content) {
                if ui.button("Break layer chain").clicked() {
                    if let Some(d) = st.project.deck_mut() {
                        d.break_layer_chain(i);
                    }
                    st.dirty = true;
                    ui.close();
                }
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

fn properties(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, _thumbs: &Thumbnailer, act: &mut Actions) {
    ui.horizontal(|ui| {
        if ui.selectable_label(st.selected == Selection::Composition, "Composition").clicked() {
            st.selected = Selection::Composition;
        }
    });
    match st.selected {
        Selection::Slot(layer, _) => {
            ui.label(RichText::new("Clip settings, effects and start / end are in the Preview panel.").small().weak());
            ui.separator();
            layer_props(ui, st, snap, layer, act);
        }
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
        if evj_media::image::is_image(&clip.path) {
            ui.label("Image duration");
            ui.add_enabled_ui(content, |ui| {
                let mut secs = clip.image_secs();
                if ui.add(egui::DragValue::new(&mut secs).range(0.1..=3600.0).speed(0.1).suffix(" s")).on_hover_text("How long the image plays (Loop repeats it, Once holds the picture at the end)").changed() {
                    clip.still_secs = Some(secs);
                }
            });
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
    let mut fx_out = crate::fxui::ChainOut::default();
    ui.add_enabled_ui(content, |ui| crate::fxui::chain(ui, "clipfx", &mut clip.effects, &snap.effects, true, &mut fx_out));
    if *clip != before {
        clip.in_point = clip.in_point.min(clip.out_point);
        let c = clip.clone();
        if playing {
            let clip = crate::chain::as_played(st, layer, col, c);
            act.commands.push(Command::UpdateClip { layer, clip });
        }
        st.dirty = true;
    }
    st.fx_rects = fx_out.rects;
    if let Some(effect) = fx_out.pick {
        st.eyedrop = Some(Eyedrop { deck: st.project.active_deck, layer, col, effect });
        st.status = "Eyedropper: click the colour to remove on the Preview or Program monitor (Esc cancels)".into();
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
    ui.add_enabled_ui(content, |ui| crate::fxui::chain(ui, "layerfx", &mut l.effects, &snap.effects, false, &mut Default::default()));
    if *l != before {
        act.commands.push(Command::SetLayer { layer, props: l.clone() });
        st.dirty = true;
    }
    if n > 1 && ui.add_enabled(content, egui::Button::new("Delete layer…")).clicked() {
        st.confirm_delete = Some(Delete::Layer(layer));
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
    if ui.add_enabled_ui(content, |ui| crate::fxui::chain(ui, "compfx", &mut st.project.composition.effects, &snap.effects, false, &mut Default::default())).inner {
        act.commands.push(Command::SetCompositionEffects(st.project.composition.effects.clone()));
        st.dirty = true;
    }
    crate::fxui::errors(ui, &snap.effects);
}

/// Stable per-layer colour (tally strips, opacity bars, countdown rows).
/// Chain i's colour (indicators on the grid and in the Chain settings).
pub fn chain_color(i: usize) -> Color32 {
    [Color32::from_rgb(80, 200, 255), Color32::from_rgb(230, 110, 255), Color32::from_rgb(150, 230, 90), Color32::from_rgb(255, 170, 60)][i % 4]
}

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
    #[test]
    fn moving_a_layer_up_moves_the_show_and_the_engine() {
        let mut st = UiState::new(Project::new_default());
        st.project.decks[0].slots[1][0] = Some(Clip::new("C:/m/b.mov".into()));
        st.trigger(1, 0, &mut Actions::default());
        st.selected = Selection::Slot(1, 0);
        let mut act = Actions::default();
        st.move_layer(1, true, &mut act);
        assert_eq!(st.project.decks[0].clip(0, 0).unwrap().name, "b");
        assert_eq!(st.playing[0], Some((0, 0)), "the tally moved with it");
        assert_eq!(st.playing[1], None);
        assert_eq!(st.selected, Selection::Slot(0, 0), "the selection follows");
        assert!(act.commands.iter().any(|c| matches!(c, Command::MoveLayer { from: 1, to: 0 })));
        assert!(!act.commands.iter().any(|c| matches!(c, Command::Trigger { .. } | Command::Clear { .. })), "nothing restarts");
        let mut act = Actions::default();
        st.move_layer(0, true, &mut act);
        assert!(act.commands.is_empty(), "layer 1 is already on top");
    }

    #[test]
    fn deleting_a_layer_keeps_the_others_playing() {
        let mut st = UiState::new(Project::new_default());
        for l in 0..3 {
            st.project.decks[0].slots[l][0] = Some(Clip::new(format!("C:/m/{l}.mov").into()));
            st.trigger(l, 0, &mut Actions::default());
        }
        let mut act = Actions::default();
        st.delete_layer(1, &mut act);
        assert_eq!(st.project.composition.layers.len(), 3);
        assert_eq!(st.playing, vec![Some((0, 0)), Some((0, 0)), None]);
        assert_eq!(st.project.decks[0].clip(1, 0).unwrap().name, "2", "layer 3 moved up");
        assert!(act.commands.iter().any(|c| matches!(c, Command::Clear { layer: 1, .. })), "the deleted layer goes dark first");
        let clears = act.commands.iter().filter(|c| matches!(c, Command::Clear { .. })).count();
        assert_eq!(clears, 1, "nothing else restarts");
        assert!(act.commands.iter().any(|c| matches!(c, Command::MoveLayer { .. })));
        assert!(act.commands.iter().any(|c| matches!(c, Command::SetLayerCount(3))));
    }

    #[test]
    fn deleting_a_scene_on_air_clears_it_first() {
        let mut st = UiState::new(Project::new_default());
        st.project.decks[0].slots[0][1] = Some(Clip::new("C:/m/b.mov".into()));
        st.project.decks[0].slots[1][3] = Some(Clip::new("C:/m/d.mov".into()));
        st.trigger(0, 1, &mut Actions::default());
        st.trigger(1, 3, &mut Actions::default());
        st.selected = Selection::Scene(3);
        let cols = st.project.columns();
        let mut act = Actions::default();
        st.delete_scene(1, &mut act);
        assert_eq!(st.project.columns(), cols - 1);
        assert!(act.commands.iter().any(|c| matches!(c, Command::Clear { layer: 0, .. })));
        assert_eq!(st.playing[0], None);
        assert_eq!(st.playing[1], Some((0, 2)), "a scene to the right moved left");
        assert_eq!(st.selected, Selection::Scene(2));
    }

    #[test]
    fn images_show_their_duration_on_the_slot() {
        let mut c = Clip::new("C:/m/photo.jpg".into());
        assert_eq!(slot_secs(&c, None), Some(evj_core::model::DEFAULT_IMAGE_SECS));
        c.still_secs = Some(7.5);
        assert_eq!(slot_secs(&c, None), Some(7.5));
        let v = Clip::new("C:/m/clip.mov".into());
        assert_eq!(slot_secs(&v, None), None, "video: from the file once it is read");
    }
}

#[cfg(test)]
mod frame_tests {
    use super::*;
    use crate::waveform::Waveforms;

    fn frame(ctx: &egui::Context, st: &mut UiState, thumbs: &mut Thumbnailer, waves: &mut Waveforms, events: Vec<egui::Event>) -> Actions {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1600.0, 900.0))), events, ..Default::default() };
        let snap = Snapshot { layers: vec![Default::default(); st.project.composition.layers.len()], ..Default::default() };
        let mut act = Actions::default();
        let _ = ctx.run_ui(raw, |ui| act.absorb(draw(ui, st, &snap, None, None, None, thumbs, waves, &Vec::new())));
        act
    }

    /// Every text drawn in one frame (menus, labels).
    fn texts(ctx: &egui::Context, st: &mut UiState, thumbs: &mut Thumbnailer, waves: &mut Waveforms, events: Vec<egui::Event>) -> Vec<String> {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1600.0, 900.0))), events, ..Default::default() };
        let snap = Snapshot { layers: vec![Default::default(); st.project.composition.layers.len()], ..Default::default() };
        let out = ctx.run_ui(raw, |ui| {
            let _ = draw(ui, st, &snap, None, None, None, thumbs, waves, &Vec::new());
        });
        out.shapes.iter().filter_map(|c| if let egui::epaint::Shape::Text(t) = &c.shape { Some(t.galley.text().to_string()) } else { None }).collect()
    }

    /// A right-click at `p` (move, press, release), then the frame after it.
    fn right_click(ctx: &egui::Context, st: &mut UiState, thumbs: &mut Thumbnailer, waves: &mut Waveforms, p: egui::Pos2) -> Vec<String> {
        let press = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Secondary, pressed, modifiers: egui::Modifiers::NONE };
        frame(ctx, st, thumbs, waves, vec![egui::Event::PointerMoved(p)]);
        frame(ctx, st, thumbs, waves, vec![press(true)]);
        frame(ctx, st, thumbs, waves, vec![press(false)]);
        texts(ctx, st, thumbs, waves, vec![])
    }

    fn key(k: egui::Key) -> egui::Event {
        egui::Event::Key { key: k, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE }
    }

    #[test]
    fn enter_answers_a_delete_question_without_a_take() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        st.selected = Selection::Slot(1, 0); // Enter would put it on Program
        let cols = st.project.columns();
        st.confirm_delete = Some(Delete::Scene(3));
        let t = texts(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        assert!(t.iter().any(|x| x == "Delete scene?"), "the question is shown: {:?}", t.iter().filter(|x| x.len() < 40).collect::<Vec<_>>());
        let act = frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![key(egui::Key::Enter)]);
        assert_eq!(st.project.columns(), cols - 1, "Enter = Delete");
        assert!(st.confirm_delete.is_none());
        assert!(!act.commands.iter().any(|c| matches!(c, Command::Trigger { .. })), "the key went to the dialog, not to TAKE");
    }

    #[test]
    fn esc_cancels_a_question() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        let cols = st.project.columns();
        st.confirm_delete = Some(Delete::Scene(3));
        frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![key(egui::Key::Escape)]);
        assert_eq!(st.project.columns(), cols);
        assert!(st.confirm_delete.is_none());
    }

    #[test]
    fn right_click_on_a_scene_header_offers_delete_and_chain() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        for c in 1..3 {
            st.project.decks[0].slots[0][c] = Some(Clip::new(format!("C:/m/{c}.png").into()));
        }
        st.multi_scenes = vec![1, 2];
        for _ in 0..3 {
            frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        }
        let p = st.scene_rects.iter().find(|(_, c)| *c == 1).map(|(r, _)| r.center()).unwrap();
        let t = right_click(&ctx, &mut st, &mut thumbs, &mut waves, p);
        for want in ["Rename…", "Delete scene…", "Chain scenes (2)"] {
            assert!(t.iter().any(|x| x == want), "{want} missing; drawn: {:?}", t.iter().filter(|x| x.len() < 30).collect::<Vec<_>>());
        }
    }

    #[test]
    fn right_click_on_a_layer_name_offers_delete() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        for _ in 0..3 {
            frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        }
        let p = st.layer_name_rects.iter().find(|(_, l)| *l == 1).map(|(r, _)| r.center()).expect("layer name");
        let t = right_click(&ctx, &mut st, &mut thumbs, &mut waves, p);
        assert!(t.iter().any(|x| x == "Delete layer…"), "drawn: {:?}", t.iter().filter(|x| x.len() < 30).collect::<Vec<_>>());
    }

    #[test]
    fn right_click_anywhere_on_a_layers_controls_opens_its_menu() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        for _ in 0..3 {
            frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        }
        // Below the name row, left of the opacity bar's end: empty space of the layer's block.
        let name = st.layer_name_rects.iter().find(|(_, l)| *l == 1).map(|(r, _)| *r).unwrap();
        let p = egui::pos2(name.left() + 2.0, name.bottom() + 70.0);
        let t = right_click(&ctx, &mut st, &mut thumbs, &mut waves, p);
        for want in ["Move up", "Move down", "Delete layer…"] {
            assert!(t.iter().any(|x| x == want), "{want} missing: {:?}", t.iter().filter(|x| x.len() < 30).collect::<Vec<_>>());
        }
    }

    #[test]
    fn chain_choices_show_even_before_a_multi_selection() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        for _ in 0..3 {
            frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        }
        let p = st.slot_rects.iter().find(|(_, l, c)| *l == 1 && *c == 0).map(|(r, _, _)| r.center()).unwrap();
        let t = right_click(&ctx, &mut st, &mut thumbs, &mut waves, p);
        assert!(t.iter().any(|x| x == "Chain…"), "{:?}", t.iter().filter(|x| x.len() < 30).collect::<Vec<_>>());
        assert!(t.iter().any(|x| x.starts_with("Shift+click")), "the menu says how");
        frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![key(egui::Key::Escape)]);
        let p = st.scene_rects.iter().find(|(_, c)| *c == 0).map(|(r, _)| r.center()).unwrap();
        let t = right_click(&ctx, &mut st, &mut thumbs, &mut waves, p);
        assert!(t.iter().any(|x| x == "Chain scenes…"), "{:?}", t.iter().filter(|x| x.len() < 30).collect::<Vec<_>>());
    }

    #[test]
    fn enter_renames_a_scene() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        st.rename_scene = Some((2, "Opening".into()));
        frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![key(egui::Key::Enter)]);
        assert!(st.rename_scene.is_none());
        assert_eq!(st.project.decks[0].scene_name(2), "Opening");
    }

    #[test]
    fn a_long_clip_name_is_cut_so_loop_and_seek_keep_their_row() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        st.layout = crate::dock::preset_live();
        st.project.decks[0].clip_mut(1, 0).unwrap().name = "VIDEO PROFIL BPIP RI 2025 - a very long clip name from a real show".into();
        st.selected = Selection::Slot(1, 0);
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1100.0, 900.0))), ..Default::default() };
        let snap = Snapshot { layers: vec![Default::default(); 4], ..Default::default() };
        let mut out = None;
        for _ in 0..4 {
            out = Some(ctx.run_ui(raw.clone(), |ui| {
                let _ = draw(ui, &mut st, &snap, None, None, None, &mut thumbs, &mut waves, &Vec::new());
            }));
        }
        let shapes: Vec<(String, Rect)> = out.unwrap().shapes.iter().filter_map(|c| if let egui::epaint::Shape::Text(t) = &c.shape { Some((t.galley.text().to_string(), t.visual_bounding_rect())) } else { None }).collect();
        let at = |f: &dyn Fn(&str) -> bool| shapes.iter().find(|(t, _)| f(t)).map(|(_, r)| *r);
        let looping = at(&|t| t == "⟲ Loop").expect("loop");
        // The timeline title: the copy of the name nearest above the Loop button (not the Preview header / clip settings).
        let name = shapes.iter().filter(|(t, r)| t.starts_with("VIDEO PROFIL") && r.top() < looping.top()).max_by(|a, b| a.1.top().total_cmp(&b.1.top())).map(|(t, r)| (t.clone(), *r)).expect("timeline title");
        let seek = at(&|t| t == "Seek:").expect("seek");
        let (_, panel) = st.preview_width;
        assert!(name.1.height() < 24.0 && name.1.width() < panel - 60.0, "one cut line inside the panel: {:?} in {panel}", name.1);
        assert!((looping.center().y - seek.center().y).abs() < 2.0, "Loop and Seek share a row");
        assert!(name.1.bottom() <= looping.top(), "the title is above them");
    }

    /// A Shift+click at `p`, as winit delivers it (Shift held in the frame's modifiers too).
    fn shift_click(ctx: &egui::Context, st: &mut UiState, thumbs: &mut Thumbnailer, waves: &mut Waveforms, p: egui::Pos2) {
        let m = egui::Modifiers::SHIFT;
        let run = |ctx: &egui::Context, st: &mut UiState, thumbs: &mut Thumbnailer, waves: &mut Waveforms, events: Vec<egui::Event>| {
            let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1600.0, 900.0))), events, modifiers: m, ..Default::default() };
            let snap = Snapshot { layers: vec![Default::default(); st.project.composition.layers.len()], ..Default::default() };
            let _ = ctx.run_ui(raw, |ui| {
                let _ = draw(ui, st, &snap, None, None, None, thumbs, waves, &Vec::new());
            });
        };
        let press = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: m };
        run(ctx, st, thumbs, waves, vec![egui::Event::PointerMoved(p)]);
        run(ctx, st, thumbs, waves, vec![press(true)]);
        run(ctx, st, thumbs, waves, vec![press(false)]);
        // Time passes between clicks (no double-click).
        std::thread::sleep(std::time::Duration::from_millis(350));
    }

    #[test]
    fn shift_clicking_three_slots_and_three_scenes_offers_both_chains() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        for l in 0..3 {
            st.project.decks[0].slots[l][1] = Some(Clip::new(format!("C:/m/{l}.png").into()));
        }
        for c in 2..4 {
            st.project.decks[0].slots[0][c] = Some(Clip::new(format!("C:/m/s{c}.png").into()));
        }
        for _ in 0..3 {
            frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        }
        let slot = |st: &UiState, l: usize| st.slot_rects.iter().find(|(_, x, c)| *x == l && *c == 1).map(|(r, _, _)| r.center()).unwrap();
        for l in 0..3 {
            let p = slot(&st, l);
            shift_click(&ctx, &mut st, &mut thumbs, &mut waves, p);
        }
        assert_eq!(st.multi, vec![(0, 1), (1, 1), (2, 1)], "three slots of the scene");
        let p = slot(&st, 2);
        let t = right_click(&ctx, &mut st, &mut thumbs, &mut waves, p);
        assert!(t.iter().any(|x| x == "Chain layers (3 clips)"), "{:?}", t.iter().filter(|x| x.len() < 30).collect::<Vec<_>>());
        frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![key(egui::Key::Escape)]);
        let head = |st: &UiState, c: usize| st.scene_rects.iter().find(|(_, x)| *x == c).map(|(r, _)| r.center()).unwrap();
        for c in 1..4 {
            let p = head(&st, c);
            shift_click(&ctx, &mut st, &mut thumbs, &mut waves, p);
        }
        assert_eq!(st.multi_scenes, vec![1, 2, 3]);
        let p = head(&st, 3);
        let t = right_click(&ctx, &mut st, &mut thumbs, &mut waves, p);
        assert!(t.iter().any(|x| x == "Chain scenes (3)"), "{:?}", t.iter().filter(|x| x.len() < 30).collect::<Vec<_>>());
    }

    #[test]
    fn slots_of_different_scenes_offer_a_scene_chain() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        st.project.decks[0].slots[1][2] = Some(Clip::new("C:/m/c.png".into()));
        for _ in 0..3 {
            frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        }
        let slot = |st: &UiState, c: usize| st.slot_rects.iter().find(|(_, l, x)| *l == 1 && *x == c).map(|(r, _, _)| r.center()).unwrap();
        for c in [0, 2] {
            let p = slot(&st, c);
            shift_click(&ctx, &mut st, &mut thumbs, &mut waves, p);
        }
        let p = slot(&st, 2);
        let t = right_click(&ctx, &mut st, &mut thumbs, &mut waves, p);
        assert!(t.iter().any(|x| x == "Chain scenes (2)"), "{:?}", t.iter().filter(|x| x.len() < 30).collect::<Vec<_>>());
    }

    /// The library as the engine reports it, with the built-in Chroma Key.
    fn fx_snap(st: &UiState) -> Snapshot {
        let meta = evj_core::effect::parse_meta(include_str!("../../../effects/chroma_key.hlsl")).unwrap();
        let info = evj_engine::fx::EffectInfo { meta, builtin: true, error: None };
        Snapshot { layers: vec![Default::default(); st.project.composition.layers.len()], effects: std::sync::Arc::new(vec![info]), ..Default::default() }
    }

    /// A tall screen: the clip effects sit below the fold of a 900 px Preview panel.
    fn frame_with(ctx: &egui::Context, st: &mut UiState, thumbs: &mut Thumbnailer, waves: &mut Waveforms, snap: &Snapshot, events: Vec<egui::Event>) -> (Actions, Vec<String>) {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1600.0, 1800.0))), events, ..Default::default() };
        let mut act = Actions::default();
        let out = ctx.run_ui(raw, |ui| act.absorb(draw(ui, st, snap, None, None, None, thumbs, waves, &Vec::new())));
        let texts = out.shapes.iter().filter_map(|c| if let egui::epaint::Shape::Text(t) = &c.shape { Some(t.galley.text().to_string()) } else { None }).collect();
        (act, texts)
    }

    fn click_at(ctx: &egui::Context, st: &mut UiState, thumbs: &mut Thumbnailer, waves: &mut Waveforms, snap: &Snapshot, p: egui::Pos2) -> Actions {
        let press = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        let mut act = frame_with(ctx, st, thumbs, waves, snap, vec![egui::Event::PointerMoved(p)]).0;
        act.absorb(frame_with(ctx, st, thumbs, waves, snap, vec![press(true)]).0);
        act.absorb(frame_with(ctx, st, thumbs, waves, snap, vec![press(false)]).0);
        act
    }

    #[test]
    fn the_eyedropper_picks_a_key_colour_from_the_preview() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        st.layout = crate::dock::preset_live();
        st.project.decks[0].clip_mut(1, 0).unwrap().effects.push(evj_core::effect::EffectRef::new("Chroma Key"));
        st.selected = Selection::Slot(1, 0);
        let snap = fx_snap(&st);
        for _ in 0..3 {
            frame_with(&ctx, &mut st, &mut thumbs, &mut waves, &snap, vec![]);
        }
        // Open the effect, then press Pick.
        let head = st.fx_rects.iter().find(|(n, _)| n == "Chroma Key").map(|(_, r)| r.center()).expect("effect header");
        click_at(&ctx, &mut st, &mut thumbs, &mut waves, &snap, head);
        for _ in 0..2 {
            frame_with(&ctx, &mut st, &mut thumbs, &mut waves, &snap, vec![]);
        }
        let pick = st.fx_rects.iter().find(|(n, _)| n == "Pick").map(|(_, r)| r.center()).unwrap_or_else(|| panic!("Pick button; header at {head:?}, rects {:?}", st.fx_rects));
        click_at(&ctx, &mut st, &mut thumbs, &mut waves, &snap, pick);
        assert!(st.eyedrop.is_some(), "armed");
        // Click the middle of the Preview monitor.
        let mon = st.monitor_rects[1].expect("preview monitor");
        let act = click_at(&ctx, &mut st, &mut thumbs, &mut waves, &snap, mon.center());
        let reply = act.commands.iter().find_map(|c| match c {
            Command::PickColor { at, pos, reply } => Some((*at, *pos, reply.clone())),
            _ => None,
        });
        let (at, pos, reply) = reply.expect("a PickColor for the engine");
        assert_eq!(at, evj_engine::PickAt::Preview);
        assert!((pos[0] - 0.5).abs() < 0.02 && (pos[1] - 0.5).abs() < 0.02, "{pos:?}");
        assert!(st.eyedrop.is_none(), "one pick per press");
        reply.send(Some([0.1, 0.8, 0.3])).unwrap();
        frame_with(&ctx, &mut st, &mut thumbs, &mut waves, &snap, vec![]);
        let e = &st.project.decks[0].clip(1, 0).unwrap().effects[0];
        let v = |n: &str| e.params.iter().find(|p| p.name == n).map(|p| p.value).unwrap();
        assert!((v("key_r") - 0.1).abs() < 1e-6 && (v("key_g") - 0.8).abs() < 1e-6 && (v("key_b") - 0.3).abs() < 1e-6);
        assert!(st.dirty);
    }

    #[test]
    fn right_click_on_chosen_slots_offers_a_layer_chain() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        st.project.decks[0].slots[0][0] = Some(Clip::new("C:/m/b.png".into()));
        st.multi = vec![(0, 0), (1, 0)];
        for _ in 0..3 {
            frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        }
        let p = st.slot_rects.iter().find(|(_, l, c)| *l == 1 && *c == 0).map(|(r, _, _)| r.center()).unwrap();
        let t = right_click(&ctx, &mut st, &mut thumbs, &mut waves, p);
        assert!(t.iter().any(|x| x == "Chain layers (2 clips)"), "drawn: {:?}", t.iter().filter(|x| x.len() < 30).collect::<Vec<_>>());
    }

    fn setup() -> (egui::Context, UiState, Thumbnailer, Waveforms) {
        let mut st = UiState::new(Project::new_default());
        st.project.decks[0].slots[1][0] = Some(Clip::new("C:/m/a.png".into()));
        (egui::Context::default(), st, Thumbnailer::start(), Waveforms::start())
    }

    #[test]
    fn scene_headers_line_up_with_their_slots() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        for _ in 0..3 {
            frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        }
        assert!(!st.scene_rects.is_empty());
        for (r, c) in &st.scene_rects {
            let slot = st.slot_rects.iter().find(|(_, l, sc)| *l == 0 && sc == c).map(|(r, _, _)| *r).expect("slot");
            assert!((r.min.x - slot.min.x).abs() < 1.0, "scene {c}: header x {} vs slot x {}", r.min.x, slot.min.x);
        }
    }

    #[test]
    fn layer_one_is_the_top_row() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        for _ in 0..3 {
            frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        }
        let y = |l: usize| st.clear_rects.iter().find(|(_, x)| *x == l).map(|(r, _)| r.min.y).unwrap();
        assert!(y(0) < y(1) && y(1) < y(3), "layer 1 first, the last layer at the bottom");
        let slot_y = |l: usize| st.slot_rects.iter().find(|(_, x, c)| *x == l && *c == 0).map(|(r, _, _)| r.min.y).unwrap();
        assert!(slot_y(0) < slot_y(3));
        let slots_x = st.slot_rects.iter().map(|(r, _, _)| r.min.x).fold(f32::MAX, f32::min);
        for (r, l) in &st.clear_rects {
            assert!(r.max.x <= slots_x, "layer {l} controls run into the slots: {} > {slots_x}", r.max.x);
        }
    }

    #[test]
    fn the_clear_button_clears_its_layer() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        for _ in 0..3 {
            frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        }
        let p = st.clear_rects.iter().find(|(_, l)| *l == 1).map(|(r, _)| r.center()).expect("clear button");
        let mut act = frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![egui::Event::PointerMoved(p)]);
        let press = |pressed| egui::Event::PointerButton { pos: p, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        act.absorb(frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![press(true)]));
        act.absorb(frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![press(false)]));
        act.absorb(frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]));
        assert!(act.commands.iter().any(|c| matches!(c, Command::Clear { layer: 1, .. })), "no Clear for layer 1 ({} commands)", act.commands.len());
    }

    #[test]
    fn the_layer_column_stays_while_the_slots_scroll() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        for _ in 0..30 {
            st.project.add_column();
        }
        for _ in 0..3 {
            frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        }
        let clear_x = st.clear_rects[0].0.min.x;
        let slot = st.slot_rects.iter().find(|(_, l, c)| *l == 0 && *c == 0).map(|(r, _, _)| *r).unwrap();
        let wheel = egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta: vec2(-600.0, 0.0), phase: egui::TouchPhase::Move, modifiers: egui::Modifiers::NONE };
        frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![egui::Event::PointerMoved(slot.center())]);
        for _ in 0..5 {
            frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![wheel.clone()]);
        }
        frame(&ctx, &mut st, &mut thumbs, &mut waves, vec![]);
        let moved = st.slot_rects.iter().find(|(_, l, c)| *l == 0 && *c == 0).map_or(-1e9, |(r, _, _)| r.min.x);
        assert!(moved < slot.min.x - 100.0, "the slots scrolled: {} → {moved}", slot.min.x);
        assert!((st.clear_rects[0].0.min.x - clear_x).abs() < 0.5, "the layer column did not move");
    }

    #[test]
    fn a_narrow_preview_tab_is_not_wider_than_itself() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        st.layout = crate::dock::preset_live();
        let video = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../evj-media/tests/fixtures/av_320x240.mp4");
        let mut clip = Clip::new(video.clone());
        clip.name = "VIDEO PROFIL BPIP RI 2025 - a long clip name from a real show".into();
        st.project.decks[0].slots[1][0] = Some(clip);
        st.selected = Selection::Slot(1, 0);
        for _ in 0..300 {
            thumbs.poll(&ctx);
            if thumbs.info(&video).is_some() {
                break;
            }
            thumbs.get(&video);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(thumbs.info(&video).is_some(), "media info read");
        let raw = |w: f32| egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(w, 900.0))), ..Default::default() };
        let snap = Snapshot { layers: vec![Default::default(); 4], ..Default::default() };
        for w in [1100.0, 800.0] {
            for _ in 0..4 {
                let _ = ctx.run_ui(raw(w), |ui| {
                    let _ = draw(ui, &mut st, &snap, None, None, None, &mut thumbs, &mut waves, &Vec::new());
                });
            }
            let (used, avail) = st.preview_width;
            assert!(avail > 50.0, "the Preview panel was drawn ({avail})");
            assert!(used <= avail + 1.0, "window {w}: Preview content {used} px wide in a {avail} px tab");
        }
    }

    #[test]
    fn chain_settings_fit_a_narrow_preview_tab() {
        let (ctx, mut st, mut thumbs, mut waves) = setup();
        st.layout = crate::dock::preset_live();
        for l in 0..3 {
            st.project.decks[0].slots[l][1] = Some(Clip::new(format!("C:/m/{l}.png").into()));
        }
        st.project.decks[0].slots[0][2] = Some(Clip::new("C:/m/x.png".into()));
        st.project.decks[0].make_layer_chain(1, &[0, 1, 2]);
        st.project.decks[0].layer_chains[0].steps[1] = evj_core::model::ChainStep { layer: 1, start: StepStart::AfterSecs(10.0), mode: StepMode::Overlay };
        st.project.decks[0].make_scene_chain(&[1, 2]);
        st.selected = Selection::Slot(1, 1);
        let raw = |w: f32| egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(w, 900.0))), ..Default::default() };
        let snap = Snapshot { layers: vec![Default::default(); 4], ..Default::default() };
        for w in [1100.0, 800.0] {
            for _ in 0..4 {
                let _ = ctx.run_ui(raw(w), |ui| {
                    let _ = draw(ui, &mut st, &snap, None, None, None, &mut thumbs, &mut waves, &Vec::new());
                });
            }
            let (used, avail) = st.preview_width;
            assert!(avail > 50.0, "the Preview panel was drawn ({avail})");
            assert!(used <= avail + 1.0, "window {w}: Preview content {used} px wide in a {avail} px tab");
        }
    }

    #[test]
    fn the_symbols_the_ui_draws_exist_in_its_fonts() {
        let ctx = egui::Context::default();
        let _ = ctx.run_ui(egui::RawInput::default(), |_| {});
        // has_glyph answers for the fonts loaded so far: a yes is sure, a no is not (✔ says no
        // and draws fine) — so only the symbols this UI added, each one checked yes.
        for c in ['⏶', '⏷', '🗗', '⟲', '■', '›'] {
            assert!(ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::proportional(12.0), c)), "{c} would show as an empty box");
        }
    }
}
