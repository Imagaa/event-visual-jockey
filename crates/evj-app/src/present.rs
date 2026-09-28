//! Presentations: PPTX / PDF import into grid slots, clicker, presenter pointer, presenter view.
//! The presentation that is "on" is the topmost Program layer showing a deck.
use crate::thumbs::{Thumb, Thumbnailer};
use crate::ui::{Actions, UiState};
use egui::{Color32, RichText, vec2};
use evj_core::model::Clip;
use evj_core::slides::{SlideDeck, SlidePos};
use evj_core::transition::{TransitionPreset, resolve_preset};
use evj_engine::{Command, Pointer, Snapshot};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use winit::keyboard::KeyCode;

/// What a presenter clicker (or the keyboard) asks for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Key {
    Next,
    Prev,
    Hide,
    /// Cycles the pointer: off → laser dot → spotlight.
    Pointer,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PointerMode {
    #[default]
    Off,
    Dot,
    Spotlight,
}

pub struct PresentState {
    /// The deck on Program: (layer, deck.json, deck).
    pub deck: Option<(usize, PathBuf, SlideDeck)>,
    started: Option<Instant>,
    /// A deck.json that failed to load (not retried every frame).
    failed: Option<PathBuf>,
    /// Deck of a presentation slot shown in the timeline while it is not on Program.
    browse: Option<(PathBuf, SlideDeck)>,
    /// Clicker presses (raw input) waiting for the next UI frame.
    pub keys: Vec<Key>,
    pub pointer: PointerMode,
    pub pointer_pos: (f32, f32),
    pub pointer_size: f32,
    /// Pointer follows the mouse / a laser clicker's gyro (raw mouse motion).
    pub follow_mouse: bool,
    pub sensitivity: f32,
    /// Mouse is over a preview picture: that sets the pointer directly.
    pub hover_preview: bool,
    sent: Option<(usize, Pointer)>,
    /// PageUp / PageDown work while another program has the focus.
    pub background_clicker: bool,
    pub show_presenter: bool,
}

impl Default for PresentState {
    fn default() -> Self {
        PresentState {
            deck: None,
            started: None,
            failed: None,
            browse: None,
            keys: Vec::new(),
            pointer: PointerMode::Off,
            pointer_pos: (0.5, 0.5),
            pointer_size: 0.03,
            follow_mouse: true,
            sensitivity: 1.0,
            hover_preview: false,
            sent: None,
            background_clicker: true,
            show_presenter: false,
        }
    }
}

impl PresentState {
    pub fn active(&self) -> bool {
        self.deck.is_some()
    }

    /// Raw mouse motion (pixels) → pointer position.
    pub fn mouse_delta(&mut self, dx: f64, dy: f64) {
        if self.pointer == PointerMode::Off || !self.follow_mouse || self.hover_preview || !self.active() {
            return;
        }
        let s = self.sensitivity as f64 / 1200.0;
        self.pointer_pos.0 = (self.pointer_pos.0 as f64 + dx * s).clamp(0.0, 1.0) as f32;
        self.pointer_pos.1 = (self.pointer_pos.1 as f64 + dy * s * 16.0 / 9.0).clamp(0.0, 1.0) as f32;
    }
}

/// Clicker mapping. In the background only PageUp / PageDown (what clickers send) count, so typing
/// in another program never changes slides.
pub fn clicker_key(code: KeyCode, focused: bool) -> Option<Key> {
    match code {
        KeyCode::PageDown => Some(Key::Next),
        KeyCode::PageUp => Some(Key::Prev),
        _ if !focused => None,
        KeyCode::ArrowRight | KeyCode::ArrowDown | KeyCode::Space | KeyCode::Enter | KeyCode::NumpadEnter => Some(Key::Next),
        KeyCode::ArrowLeft | KeyCode::ArrowUp | KeyCode::Backspace => Some(Key::Prev),
        KeyCode::KeyB | KeyCode::Period => Some(Key::Hide),
        KeyCode::KeyL => Some(Key::Pointer),
        _ => None,
    }
}

/// Keys the clicker path owns while presenting (the keymap must not also see them).
pub fn is_clicker_key(k: egui::Key) -> bool {
    use egui::Key::*;
    matches!(k, PageDown | PageUp | ArrowRight | ArrowDown | ArrowLeft | ArrowUp | Space | Enter | Backspace | B | Period | L)
}

fn slide_transition(st: &UiState, layer: usize) -> Option<TransitionPreset> {
    let l = st.project.composition.layers.get(layer).cloned().unwrap_or_default();
    resolve_preset(&st.project.transitions, &Clip::default(), st.next_transition.as_deref(), &l).cloned()
}

/// Layer of the presentation on Program.
fn deck_layer(st: &UiState) -> Option<usize> {
    st.present.deck.as_ref().map(|(l, _, _)| *l)
}

fn set_hidden(st: &mut UiState, hidden: bool, act: &mut Actions) {
    let Some(layer) = deck_layer(st) else { return };
    if let Some(l) = st.project.composition.layers.get_mut(layer).filter(|l| l.bypass != hidden) {
        l.bypass = hidden;
        act.commands.push(Command::SetLayer { layer, props: l.clone() });
        st.dirty = true;
    }
}

fn hidden(st: &UiState) -> bool {
    deck_layer(st).and_then(|l| st.project.composition.layers.get(l)).is_some_and(|l| l.bypass)
}

fn press(st: &mut UiState, key: Key, act: &mut Actions) {
    let Some(layer) = deck_layer(st) else { return };
    match key {
        Key::Next => {
            set_hidden(st, false, act);
            act.commands.push(Command::SlideNext { layer, transition: slide_transition(st, layer) });
        }
        Key::Prev => {
            set_hidden(st, false, act);
            act.commands.push(Command::SlidePrev { layer, transition: slide_transition(st, layer) });
        }
        Key::Hide => {
            let h = !hidden(st);
            set_hidden(st, h, act);
        }
        Key::Pointer => {
            st.present.pointer = match st.present.pointer {
                PointerMode::Off => PointerMode::Dot,
                PointerMode::Dot => PointerMode::Spotlight,
                PointerMode::Spotlight => PointerMode::Off,
            };
        }
    }
}

pub fn goto(st: &mut UiState, slide: usize, act: &mut Actions) {
    let Some(layer) = deck_layer(st) else { return };
    set_hidden(st, false, act);
    act.commands.push(Command::SlideGoto { layer, slide, transition: slide_transition(st, layer) });
}

/// Where the deck on Program is: (slide, step, count).
fn position(st: &UiState, snap: &Snapshot) -> Option<(usize, usize, usize)> {
    deck_layer(st).and_then(|l| snap.layers.get(l)).and_then(|l| l.slide)
}

/// The topmost layer on Program that shows a presentation.
pub fn program_deck_layer(snap: &Snapshot) -> Option<usize> {
    snap.layers.iter().rposition(|l| l.slide.is_some())
}

/// Every frame: clicker presses, which deck is on Program, pointer to the engine.
pub fn update(st: &mut UiState, snap: &Snapshot, act: &mut Actions) {
    for k in std::mem::take(&mut st.present.keys) {
        press(st, k, act);
    }
    let on = program_deck_layer(snap).and_then(|l| Some((l, snap.layers[l].clip_path.clone()?)));
    match on {
        Some((l, path)) => {
            let same = st.present.deck.as_ref().is_some_and(|(dl, p, _)| *dl == l && *p == path);
            if !same && st.present.failed.as_ref() != Some(&path) {
                match SlideDeck::load(&path) {
                    Ok(d) => {
                        st.present.deck = Some((l, path, d));
                        st.present.started = Some(Instant::now());
                        st.present.failed = None;
                    }
                    Err(e) => {
                        st.status = format!("Cannot open {}: {e:#}", path.display());
                        st.present.failed = Some(path);
                    }
                }
            }
        }
        None => {
            // Keep it while its layer is still loading the next slide.
            let loading = deck_layer(st).and_then(|l| snap.layers.get(l)).is_some_and(|l| l.loading);
            if st.present.deck.is_some() && !loading {
                st.present.deck = None;
                st.present.pointer = PointerMode::Off;
            }
        }
    }
    let layer = deck_layer(st).unwrap_or(0);
    let p = &st.present;
    let want = (p.active() && p.pointer != PointerMode::Off).then(|| Pointer {
        x: p.pointer_pos.0,
        y: p.pointer_pos.1,
        spotlight: p.pointer == PointerMode::Spotlight,
        size: p.pointer_size,
    });
    let sent = st.present.sent;
    let changed = match (sent, want) {
        (None, None) => false,
        (Some((l, a)), Some(b)) => l != layer || a != b,
        _ => true,
    };
    if changed {
        if let Some((old, _)) = sent.filter(|(l, _)| *l != layer || want.is_none()) {
            act.commands.push(Command::SetPointer { layer: old, pointer: None });
        }
        if want.is_some() {
            act.commands.push(Command::SetPointer { layer, pointer: want });
        }
        st.present.sent = want.map(|w| (layer, w));
    }
}

fn clock(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

pub fn local_time() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:02}:{:02}", t.wHour, t.wMinute)
}

/// Picture of the output; hovering it aims the pointer.
fn aim_preview(ui: &mut egui::Ui, st: &mut UiState, live: Option<(egui::TextureId, [f32; 2])>, width: f32) {
    let Some((id, size)) = live else {
        ui.label("preview unavailable");
        return;
    };
    let r = ui.add(egui::Image::new((id, vec2(width, width * size[1] / size[0]))).sense(egui::Sense::hover()));
    let aiming = st.present.pointer != PointerMode::Off && st.present.active();
    st.present.hover_preview |= aiming && r.hovered();
    if let Some(pos) = r.hover_pos().filter(|_| aiming) {
        let rel = (pos - r.rect.min) / r.rect.size();
        st.present.pointer_pos = (rel.x.clamp(0.0, 1.0), rel.y.clamp(0.0, 1.0));
    }
}

fn pointer_controls(ui: &mut egui::Ui, st: &mut UiState) {
    ui.horizontal(|ui| {
        ui.label("Pointer").on_hover_text("L cycles off / laser dot / spotlight");
        let p = &mut st.present;
        for (m, label) in [(PointerMode::Off, "Off"), (PointerMode::Dot, "Laser dot"), (PointerMode::Spotlight, "Spotlight")] {
            ui.selectable_value(&mut p.pointer, m, label);
        }
        ui.add(egui::Slider::new(&mut p.pointer_size, 0.01..=0.3).show_value(false)).on_hover_text("Size");
    });
}

/// Slide controls in the timeline for a presentation clip (the selected slot).
pub fn timeline_controls(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, thumbs: &mut Thumbnailer, layer: usize, col: usize, act: &mut Actions) {
    let Some(clip) = st.project.deck().and_then(|d| d.clip(layer, col)).cloned() else { return };
    let on_air = st.is_playing(layer, col) && deck_layer(st) == Some(layer);
    let deck = if on_air {
        st.present.deck.as_ref().map(|(_, _, d)| d.clone())
    } else {
        if st.present.browse.as_ref().is_none_or(|(p, _)| *p != clip.path) {
            st.present.browse = SlideDeck::load(&clip.path).ok().map(|d| (clip.path.clone(), d));
        }
        st.present.browse.as_ref().map(|(_, d)| d.clone())
    };
    let Some(deck) = deck else {
        ui.label(RichText::new(format!("{} — the slides cannot be read (import again?)", clip.name)).color(Color32::LIGHT_RED));
        return;
    };
    let (slide, step, n) = if on_air { position(st, snap).unwrap_or((0, 0, deck.slides.len())) } else { (0, 0, deck.slides.len()) };
    let steps = deck.slides.get(slide).map_or(1, |s| s.steps.len().max(1));
    ui.horizontal(|ui| {
        ui.strong(&clip.name);
        if on_air {
            ui.label(RichText::new("PRESENTING").color(Color32::from_rgb(235, 60, 60)).strong());
            ui.strong(format!("Slide {} / {n}", slide + 1));
            if steps > 1 {
                ui.label(format!("click {}/{steps}", step + 1));
            }
            ui.label(clock(st.present.started.map_or(0, |t| t.elapsed().as_secs())));
            if hidden(st) {
                ui.colored_label(Color32::from_rgb(255, 150, 40), "HIDDEN");
            }
            ui.separator();
            if ui.button("◀ Prev").clicked() {
                press(st, Key::Prev, act);
            }
            if ui.button("Next ▶").clicked() {
                press(st, Key::Next, act);
            }
            if ui.selectable_label(hidden(st), "Hide (B)").on_hover_text("Take the slides off the screen; the layers below stay").clicked() {
                press(st, Key::Hide, act);
            }
            ui.toggle_value(&mut st.present.show_presenter, "Presenter view");
        } else {
            ui.label(RichText::new(format!("{n} slides — double-click or TAKE to present")).weak());
        }
    });
    if on_air {
        ui.horizontal(|ui| {
            pointer_controls(ui, st);
            ui.checkbox(&mut st.present.follow_mouse, "Follow mouse / laser clicker");
            ui.add(egui::Slider::new(&mut st.present.sensitivity, 0.2..=4.0).text("speed"));
            ui.checkbox(&mut st.present.background_clicker, "Clicker in the background (PageUp / PageDown)");
        });
    }
    ui.horizontal(|ui| {
        egui::ScrollArea::horizontal().id_salt("slide_strip").max_width(ui.available_width() * 0.6).show(ui, |ui| {
            ui.horizontal(|ui| {
                for (i, s) in deck.slides.iter().enumerate() {
                    let r = match thumbs.get(&s.image) {
                        Thumb::Ready { tex, .. } => ui.add(egui::Button::image(egui::Image::new((tex.id(), vec2(80.0, 45.0)))).selected(on_air && i == slide)),
                        _ => ui.add_sized(vec2(80.0, 45.0), egui::Button::new(format!("{}", i + 1)).selected(on_air && i == slide)),
                    };
                    if r.on_hover_text(format!("Slide {}", i + 1)).clicked() && on_air {
                        goto(st, i, act);
                    }
                }
            });
        });
        if let Some(notes) = deck.slides.get(slide).map(|s| s.notes.trim()).filter(|n| !n.is_empty()) {
            egui::ScrollArea::vertical().id_salt("notes").max_height(70.0).show(ui, |ui| ui.label(notes));
        }
    });
}

/// A background import finished: the slot now plays the deck, or remembers the error.
pub fn imported(st: &mut UiState, src: &Path, result: Result<PathBuf, String>) {
    let at = st.importing.remove(src);
    match result {
        Ok(deck) => {
            let title = SlideDeck::load(&deck).ok().map(|d| d.title).filter(|t| !t.trim().is_empty());
            let name = title.unwrap_or_else(|| src.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
            let mut n = 0;
            let target = at.and_then(|(d, l, c)| st.project.decks.get_mut(d)?.clip_mut(l, c)).filter(|c| c.path == src);
            match target {
                Some(c) => {
                    c.path = deck.clone();
                    c.name = name.clone();
                    n = 1;
                }
                None => {
                    for c in st.project.decks.iter_mut().flat_map(|d| d.slots.iter_mut().flatten().flatten()).filter(|c| c.path == src) {
                        c.path = deck.clone();
                        c.name = name.clone();
                        n += 1;
                    }
                }
            }
            st.import_errors.remove(src);
            st.dirty |= n > 0;
            st.status = format!("Imported \"{name}\" — ready in the grid");
        }
        Err(e) => {
            st.status = format!("Import of {} failed: {e}", src.display());
            st.import_errors.insert(src.to_path_buf(), e);
        }
    }
}

/// Full-size slide pictures for the presenter window (its own egui context).
pub struct SlideImages {
    tx: Sender<PathBuf>,
    rx: Receiver<(PathBuf, Option<egui::ColorImage>)>,
    cache: HashMap<PathBuf, Option<egui::TextureHandle>>,
}

impl SlideImages {
    pub fn start() -> SlideImages {
        let (tx, jobs) = channel::<PathBuf>();
        let (done, rx) = channel();
        let _ = std::thread::Builder::new().name("evj-slide-images".into()).spawn(move || {
            for path in jobs {
                let img = evj_media::image::load_image(&path, 1280).ok().map(|(w, h, mut px)| {
                    for p in px.chunks_exact_mut(4) {
                        p.swap(0, 2); // BGRA → RGBA
                    }
                    egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px)
                });
                if done.send((path, img)).is_err() {
                    break;
                }
            }
        });
        SlideImages { tx, rx, cache: HashMap::new() }
    }

    fn get(&mut self, ctx: &egui::Context, path: &Path) -> Option<egui::TextureId> {
        while let Ok((p, img)) = self.rx.try_recv() {
            let tex = img.map(|i| ctx.load_texture(p.to_string_lossy(), i, egui::TextureOptions::LINEAR));
            self.cache.insert(p, tex);
        }
        if !self.cache.contains_key(path) {
            let _ = self.tx.send(path.to_path_buf());
            self.cache.insert(path.to_path_buf(), None);
        }
        self.cache[path].as_ref().map(|t| t.id())
    }
}

fn fitted(ui: &mut egui::Ui, id: Option<egui::TextureId>, aspect: f32, max: egui::Vec2) {
    let size = if max.x / max.y > aspect { vec2(max.y * aspect, max.y) } else { vec2(max.x, max.x / aspect) };
    match id {
        Some(id) => {
            ui.image((id, size));
        }
        None => {
            let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            ui.painter().rect_filled(r, 4.0, Color32::from_gray(25));
        }
    }
}

/// Presenter view (second window): live output, next slide, notes, timer, clock.
pub fn presenter(ui: &mut egui::Ui, st: &mut UiState, snap: &Snapshot, live: Option<(egui::TextureId, [f32; 2])>, images: &mut SlideImages, act: &mut Actions) {
    egui::CentralPanel::default().show(ui, |ui| {
        let Some((layer, _, deck)) = st.present.deck.clone() else {
            ui.centered_and_justified(|ui| ui.heading("No presentation on Program — double-click a presentation in the grid."));
            return;
        };
        let (slide, step, n) = position(st, snap).unwrap_or((0, 0, deck.slides.len()));
        let from_grid = st.playing.get(layer).copied().flatten().and_then(|(d, c)| st.project.decks.get(d)?.clip(layer, c)).map(|c| c.name.clone());
        let title = from_grid.unwrap_or_else(|| deck.title.clone());
        ui.horizontal(|ui| {
            ui.heading(RichText::new(format!("{title}   ·   Slide {} / {n}", slide + 1)).size(26.0));
            if hidden(st) {
                ui.add_space(20.0);
                ui.label(RichText::new("SLIDES HIDDEN").size(22.0).color(Color32::from_rgb(255, 150, 40)));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.heading(RichText::new(local_time()).size(30.0));
                ui.add_space(30.0);
                ui.heading(RichText::new(clock(st.present.started.map_or(0, |t| t.elapsed().as_secs()))).size(30.0).color(Color32::from_rgb(255, 200, 80)));
            });
        });
        ui.separator();
        let aspect = deck.width.max(1) as f32 / deck.height.max(1) as f32;
        let avail = ui.available_size();
        let bottom = 70.0;
        ui.horizontal(|ui| {
            let w = avail.x * 0.6;
            ui.vertical(|ui| {
                ui.label("Now (output)");
                let a = live.map_or(aspect, |(_, s)| s[0] / s[1]);
                let h = (avail.y - bottom - 30.0).max(100.0);
                let size = if w / h > a { vec2(h * a, h) } else { vec2(w, w / a) };
                aim_preview(ui, st, live, size.x);
            });
            ui.vertical(|ui| {
                let next = SlideDeck::next(&deck, SlidePos { slide, step });
                let label = match next {
                    None => "Next: end of presentation".to_string(),
                    Some(p) if p.slide == slide => format!("Next: click {} on this slide", p.step + 1),
                    Some(p) => format!("Next: slide {}", p.slide + 1),
                };
                ui.label(label);
                let tex = next.filter(|p| p.slide != slide).and_then(|p| deck.slides.get(p.slide)).and_then(|s| images.get(ui.ctx(), &s.image));
                fitted(ui, tex, aspect, vec2(avail.x * 0.38, avail.y * 0.35));
                ui.separator();
                ui.label("Notes");
                let notes = deck.slides.get(slide).map(|s| s.notes.clone()).unwrap_or_default();
                egui::ScrollArea::vertical().id_salt("pv_notes").max_height((avail.y * 0.5 - bottom).max(60.0)).show(ui, |ui| {
                    ui.label(RichText::new(if notes.trim().is_empty() { "(no notes)" } else { notes.trim() }).size(22.0));
                });
            });
        });
        ui.separator();
        ui.horizontal(|ui| {
            let big = |t: &str| egui::Button::new(RichText::new(t).size(24.0)).min_size(vec2(150.0, 48.0));
            if ui.add(big("◀ Prev")).clicked() {
                press(st, Key::Prev, act);
            }
            if ui.add(big("Next ▶")).clicked() {
                press(st, Key::Next, act);
            }
            if ui.add(big(if hidden(st) { "Show" } else { "Hide" })).clicked() {
                press(st, Key::Hide, act);
            }
            ui.add_space(20.0);
            ui.vertical(|ui| pointer_controls(ui, st));
        });
    });
}

/// Background PPTX / PDF imports, each in its own `evj-import.exe` process (Office can hang, and
/// Windows' PDF renderer crashes on unload — neither may take the show down).
pub struct Imports {
    list: Vec<Import>,
}

struct Import {
    input: PathBuf,
    started: Instant,
    result: Arc<Mutex<Option<Result<PathBuf, String>>>>,
    cancel: Arc<AtomicBool>,
}

pub enum Imported {
    Ok { input: PathBuf, deck: PathBuf },
    Failed { input: PathBuf, error: String },
}

/// Office exports every slide's animations as video: large decks take minutes.
const IMPORT_TIMEOUT: Duration = Duration::from_secs(45 * 60);

fn importer() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let p = exe.with_file_name("evj-import.exe");
    if p.exists() { Ok(p) } else { Err(format!("{} is missing (install EVJ again)", p.display())) }
}

/// A new folder for the imported deck: `<project>\decks\<name>` or `%APPDATA%\EVJ\decks\<name>`.
pub fn deck_dir(project: Option<&Path>, input: &Path) -> Option<PathBuf> {
    let base = match project.and_then(Path::parent) {
        Some(p) => p.join("decks"),
        None => evj_core::io::app_dir()?.join("decks"),
    };
    let stem = input.file_stem()?.to_string_lossy().into_owned();
    (1..1000).map(|n| if n == 1 { base.join(&stem) } else { base.join(format!("{stem} ({n})")) }).find(|d| !d.exists())
}

impl Imports {
    pub fn new() -> Imports {
        Imports { list: Vec::new() }
    }

    pub fn start(&mut self, input: PathBuf, out: PathBuf, width: u32, height: u32) -> Result<(), String> {
        let kind = match input.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).as_deref() {
            Some("pdf") => "pdf",
            Some("pptx" | "ppt" | "pptm" | "ppsx" | "pps" | "odp") => "pptx",
            _ => return Err("only PowerPoint and PDF files can be imported".into()),
        };
        if self.list.iter().any(|j| j.input == input) {
            return Err("already importing".into());
        }
        let exe = importer()?;
        let result = Arc::new(Mutex::new(None));
        let cancel = Arc::new(AtomicBool::new(false));
        let (r, c, i) = (result.clone(), cancel.clone(), input.clone());
        std::thread::Builder::new()
            .name("evj-import".into())
            .spawn(move || {
                let outcome = run_import(&exe, kind, &i, &out, width, height, &c);
                if let Ok(mut s) = r.lock() {
                    *s = Some(outcome);
                }
            })
            .map_err(|e| e.to_string())?;
        self.list.push(Import { input, started: Instant::now(), result, cancel });
        Ok(())
    }

    pub fn cancel_all(&self) {
        for j in &self.list {
            j.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn status(&self) -> Vec<String> {
        self.list
            .iter()
            .map(|j| {
                let name = j.input.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                format!("Importing {name} {}", clock(j.started.elapsed().as_secs()))
            })
            .collect()
    }

    pub fn poll(&mut self) -> Vec<Imported> {
        let mut done = Vec::new();
        self.list.retain(|j| match j.result.lock().ok().and_then(|mut s| s.take()) {
            Some(Ok(deck)) => {
                done.push(Imported::Ok { input: j.input.clone(), deck });
                false
            }
            Some(Err(error)) => {
                done.push(Imported::Failed { input: j.input.clone(), error });
                false
            }
            None => true,
        });
        done
    }
}

fn run_import(exe: &Path, kind: &str, input: &Path, out: &Path, width: u32, height: u32, cancel: &AtomicBool) -> Result<PathBuf, String> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut child = std::process::Command::new(exe)
        .args([kind.as_ref(), input.as_os_str(), out.as_os_str(), width.to_string().as_ref(), height.to_string().as_ref()])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("cannot start the importer: {e}"))?;
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        if let Some(o) = stdout.as_mut() {
            let _ = o.read_to_string(&mut s);
        }
        s
    });
    let started = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().map_err(|e| e.to_string())? {
            break s;
        }
        if cancel.load(Ordering::Relaxed) || started.elapsed() > IMPORT_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            return Err(if cancel.load(Ordering::Relaxed) { "cancelled".into() } else { "took too long (over 45 minutes) and was stopped".into() });
        }
        std::thread::sleep(Duration::from_millis(200));
    };
    let text = reader.join().unwrap_or_default();
    let deck = out.join(evj_core::slides::MANIFEST);
    if status.success() && deck.exists() {
        Ok(deck)
    } else {
        let msg = text.trim();
        Err(if msg.is_empty() { format!("the importer stopped ({status})") } else { msg.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicker_keys() {
        assert_eq!(clicker_key(KeyCode::PageDown, false), Some(Key::Next));
        assert_eq!(clicker_key(KeyCode::PageUp, false), Some(Key::Prev));
        assert_eq!(clicker_key(KeyCode::ArrowRight, false), None, "arrows in another program never change slides");
        assert_eq!(clicker_key(KeyCode::ArrowRight, true), Some(Key::Next));
        assert_eq!(clicker_key(KeyCode::KeyB, true), Some(Key::Hide));
        assert_eq!(clicker_key(KeyCode::KeyL, true), Some(Key::Pointer));
        assert_eq!(clicker_key(KeyCode::KeyL, false), None);
        assert_eq!(clicker_key(KeyCode::KeyA, true), None);
    }

    #[test]
    fn pointer_follows_mouse_only_when_on() {
        let mut p = PresentState::default();
        p.mouse_delta(600.0, 0.0);
        assert_eq!(p.pointer_pos, (0.5, 0.5), "pointer off");
        p.deck = Some((0, PathBuf::new(), SlideDeck { title: String::new(), source: PathBuf::new(), width: 1, height: 1, slides: vec![] }));
        p.pointer = PointerMode::Dot;
        p.mouse_delta(600.0, 0.0);
        assert_eq!(p.pointer_pos, (1.0, 0.5));
        p.mouse_delta(-6000.0, -6000.0);
        assert_eq!(p.pointer_pos, (0.0, 0.0), "clamped");
    }

    #[test]
    fn the_topmost_program_deck_owns_the_clicker() {
        let mut s = Snapshot { layers: vec![Default::default(); 4], ..Default::default() };
        assert_eq!(program_deck_layer(&s), None);
        s.layers[1].slide = Some((0, 0, 3));
        s.layers[3].slide = Some((2, 0, 5));
        assert_eq!(program_deck_layer(&s), Some(3));
    }

    #[test]
    fn a_finished_import_turns_the_slot_into_the_deck() {
        let mut st = UiState::new(evj_core::model::Project::new_default());
        let src = PathBuf::from("C:/talks/Keynote.pptx");
        let placed = st.place(1, 0, std::slice::from_ref(&src));
        assert_eq!(placed, vec![src.clone()], "presentations are reported for import");
        assert!(st.importing.contains_key(&src));
        imported(&mut st, &src, Ok(PathBuf::from("C:/show/decks/Keynote/deck.json")));
        let c = st.project.decks[0].clip(1, 0).unwrap();
        assert_eq!(c.path, PathBuf::from("C:/show/decks/Keynote/deck.json"));
        assert_eq!(c.name, "Keynote");
        assert!(st.importing.is_empty());
        let bad = PathBuf::from("C:/talks/Other.pdf");
        st.place(1, 1, std::slice::from_ref(&bad));
        imported(&mut st, &bad, Err("boom".into()));
        assert_eq!(st.import_errors.get(&bad).map(String::as_str), Some("boom"));
        assert_eq!(st.project.decks[0].clip(1, 1).unwrap().path, bad, "the slot keeps the source for a retry");
    }

    #[test]
    fn deck_dirs_do_not_overwrite() {
        let d = std::env::temp_dir().join(format!("evj-deckdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let project = d.join("show.vjproj");
        let first = deck_dir(Some(&project), Path::new("C:/x/Talk.pptx")).unwrap();
        assert_eq!(first, d.join("decks").join("Talk"));
        std::fs::create_dir_all(&first).unwrap();
        assert_eq!(deck_dir(Some(&project), Path::new("C:/x/Talk.pptx")).unwrap(), d.join("decks").join("Talk (2)"));
    }
}
