//! EVJ — show-control visuals. `evj [--bench SECONDS | --soak HOURS] [--output MONITOR] [project.vjproj | clip...]`
// Release builds are a window app: no console window next to EVJ (see `attach_console`).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod audioset;
mod chain;
mod crash;
mod dialogs;
mod dock;
mod fxui;
mod jobs;
mod lock;
mod meters;
mod modal;
mod outputs;
mod outui;
mod present;
mod shortcuts;
mod preview;
mod soak;
mod thumbs;
mod timeline;
mod transport;
mod trui;
mod ui;
mod waveform;
mod welcome;
mod widgets;

use anyhow::{Context, Result};
use evj_core::io;
use evj_core::model::Project;
use evj_engine::{Command, Engine, EngineConfig};
use evj_render::{DeviceKind, Gpu, Swapchain};
use preview::Preview;
use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};
use thumbs::Thumbnailer;
use ui::{Actions, Menu, UiState};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;
use winit::application::ApplicationHandler;
use std::collections::HashSet;
use winit::event::{DeviceEvent, DeviceId, ElementState, RawKeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, DeviceEvents, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
use winit::window::{Window, WindowId};

#[unsafe(no_mangle)]
#[used]
pub static NvOptimusEnablement: u32 = 1;
#[unsafe(no_mangle)]
#[used]
pub static AmdPowerXpressRequestHighPerformance: i32 = 1;

/// Seconds ignored at bench start (clip open, first decode).
const WARMUP: f64 = 2.0;
const AUTOSAVE_EVERY: Duration = Duration::from_secs(60);

struct Args {
    bench: Option<f64>,
    soak: Option<f64>,
    output_monitor: Option<usize>,
    files: Vec<PathBuf>,
}

fn parse_args() -> Result<Args> {
    let mut a = Args { bench: None, soak: None, output_monitor: None, files: vec![] };
    let mut it = std::env::args().skip(1);
    while let Some(x) = it.next() {
        match x.as_str() {
            "--bench" => a.bench = Some(it.next().context("--bench needs seconds")?.parse()?),
            "--soak" => a.soak = Some(it.next().context("--soak needs hours")?.parse()?),
            "--output" => a.output_monitor = Some(it.next().context("--output needs a monitor index")?.parse()?),
            _ => a.files.push(x.into()),
        }
    }
    Ok(a)
}

/// `effects/` next to the exe and in %APPDATA%\\EVJ (created so users find it).
fn effect_folders() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(|p| p.join("effects"))) {
        v.push(dir);
    }
    if let Some(app) = io::app_dir() {
        let d = app.join("effects");
        let _ = std::fs::create_dir_all(&d);
        v.push(d);
    }
    v
}

fn window_icon() -> Option<winit::window::Icon> {
    winit::window::Icon::from_rgba(include_bytes!("../../../assets/evj-icon-64.rgba").to_vec(), 64, 64).ok()
}

fn hwnd(window: &Window) -> Result<isize> {
    match window.window_handle()?.as_raw() {
        RawWindowHandle::Win32(h) => Ok(h.hwnd.get()),
        _ => anyhow::bail!("not a Win32 window"),
    }
}

/// Files in dropped folders, sorted (non-recursive).
fn expand_drops(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut files: Vec<PathBuf> = std::fs::read_dir(&p).into_iter().flatten().flatten().map(|e| e.path()).filter(|f| f.is_file()).collect();
            files.sort();
            out.extend(files);
        } else {
            out.push(p);
        }
    }
    out
}

struct UiWin {
    window: Window,
    chain: Swapchain,
    egui_ctx: egui::Context,
    egui_winit: egui_winit::State,
    egui_rend: egui_directx11::Renderer,
    /// Last present reached the screen; a minimised / covered window is paced by a timer instead.
    visible: bool,
    /// Frame timing of this window (ms): the bench prints it.
    times: UiTimes,
}

/// UI frame timing: interval between frames, time waiting for the swapchain, building the egui
/// frame (`draw`), and render + present.
#[derive(Default)]
struct UiTimes {
    interval: evj_engine::stats::FrameStats,
    wait: evj_engine::stats::FrameStats,
    draw: evj_engine::stats::FrameStats,
    lock: evj_engine::stats::FrameStats,
    render: evj_engine::stats::FrameStats,
    present: evj_engine::stats::FrameStats,
    last: Option<Instant>,
}

impl UiTimes {
    /// The moments of one frame: start, after the swapchain wait, after `draw`, after locking the
    /// shared previews, after the egui render, after present.
    fn record(&mut self, t: [Instant; 6]) {
        let ms = |a: Instant, b: Instant| (b - a).as_secs_f32() * 1000.0;
        if let Some(last) = self.last {
            self.interval.record(ms(last, t[1]), 1000.0 / 60.0);
        }
        self.last = Some(t[1]);
        for (i, s) in [&mut self.wait, &mut self.draw, &mut self.lock, &mut self.render, &mut self.present].into_iter().enumerate() {
            s.record(ms(t[i], t[i + 1]), f32::MAX);
        }
    }

    fn reset(&mut self) {
        *self = UiTimes::default();
    }

    fn report(&self) -> String {
        let row = |name: &str, s: &evj_engine::stats::FrameStats| format!("ui {name}: median {:.2} ms, p99 {:.2} ms\n", s.recent_percentile(50.0), s.recent_percentile(99.0));
        format!("ui fps: {:.1}\nui slow frames: {}\n", self.interval.recent_fps(), self.interval.dropped)
            + &row("interval", &self.interval)
            + &row("wait", &self.wait)
            + &row("draw", &self.draw)
            + &row("preview lock", &self.lock)
            + &row("render", &self.render)
            + &row("present", &self.present)
    }
}

impl UiWin {
    fn open(el: &ActiveEventLoop, gpu: &Gpu, attrs: winit::window::WindowAttributes) -> Result<UiWin> {
        let window = el.create_window(attrs.with_window_icon(window_icon()))?;
        let size = window.inner_size();
        let chain = Swapchain::new(gpu, HWND(hwnd(&window)? as _), size.width, size.height)?;
        chain.set_max_latency(2)?;
        let egui_ctx = egui::Context::default();
        let egui_winit = egui_winit::State::new(egui_ctx.clone(), egui_ctx.viewport_id(), &window, None, None, None);
        let egui_rend = egui_directx11::Renderer::new(&gpu.device)?;
        Ok(UiWin { window, chain, egui_ctx, egui_winit, egui_rend, visible: true, times: UiTimes::default() })
    }

    /// One egui frame, presented. `shared` previews are held (keyed mutex) while they are drawn.
    fn frame(&mut self, gpu: &Gpu, shared: &[Option<&Preview>], run: impl FnMut(&mut egui::Ui)) -> Result<()> {
        let start = Instant::now();
        if self.visible {
            self.chain.wait();
        } else {
            // Present returns at once when hidden: without this the UI thread spins and starves the engine.
            std::thread::sleep(Duration::from_millis(15));
        }
        let Some(rtv) = self.chain.rtv().cloned() else { return Ok(()) };
        let waited = Instant::now();
        let input = self.egui_winit.take_egui_input(&self.window);
        let out = self.egui_ctx.run_ui(input, run);
        let drawn = Instant::now();
        let (renderer_output, platform_output, _) = egui_directx11::split_output(out);
        self.egui_winit.handle_platform_output(&self.window, platform_output);
        unsafe { gpu.ctx.ClearRenderTargetView(&rtv, &[0.05, 0.05, 0.06, 1.0]) };
        let held: Vec<&Preview> = shared.iter().flatten().copied().filter(|p| p.lock()).collect();
        let locked = Instant::now();
        let r = self.egui_rend.render(&gpu.ctx, &rtv, &self.egui_ctx, renderer_output);
        for p in held {
            p.unlock();
        }
        r?;
        let rendered = Instant::now();
        self.visible = self.chain.present()?;
        self.times.record([start, waited, drawn, locked, rendered, Instant::now()]);
        Ok(())
    }
}

/// LOCK LIVE: menus that change the show's content are refused (Save is always fine).
fn menu_blocked(locked: bool, m: &Menu) -> bool {
    locked && matches!(m, Menu::New | Menu::Open | Menu::OpenRecent(_) | Menu::AddClips(..) | Menu::Relink | Menu::ImportFolder | Menu::ImportPresentation | Menu::AttachAudio(..))
}

/// Where autosave writes the show itself: only a show that already has a file and has changes.
fn autosave_target(project_path: Option<&Path>, dirty: bool) -> Option<PathBuf> {
    project_path.filter(|_| dirty).map(Path::to_path_buf)
}

/// What an open file dialog is for (its answer arrives later, see `dialog_done`).
enum Pending {
    Open,
    AddClips(usize, usize),
    /// (deck, layer, column) that gets the attached audio.
    AttachAudio(usize, usize, usize),
    Relink,
    SaveAs { quit: bool },
    ImportFolder,
}

/// The presenter's own window (confidence monitor).
struct PresenterWin {
    win: UiWin,
    live: Option<Preview>,
    images: present::SlideImages,
    last: Instant,
}

struct App {
    args: Args,
    gpu: Gpu,
    engine: Engine,
    state: UiState,
    thumbs: Thumbnailer,
    layout_gate: dock::SaveGate,
    waves: waveform::Waveforms,
    ui: Option<UiWin>,
    outputs: outputs::Outputs,
    jobs: jobs::Jobs,
    out_previews: outui::OutputPreviews,
    last_out_preview: Instant,
    seen_resets: u32,
    preview: Option<Preview>,
    tr_preview: Option<Preview>,
    /// The Preview monitor (cued clip).
    cue_preview: Option<Preview>,
    started: Instant,
    bench_reset: bool,
    /// Process CPU time and wall clock when the bench measurement started.
    bench_cpu: Option<(Duration, Instant)>,
    drops: Vec<PathBuf>,
    last_autosave: Instant,
    error: Option<anyhow::Error>,
    presenter: Option<PresenterWin>,
    imports: present::Imports,
    /// Keys held down (raw input), so auto-repeat is not a second click.
    keys_down: HashSet<KeyCode>,
    /// An EVJ control window (main or presenter) has the keyboard focus.
    focused: bool,
    soak: Option<soak::Soak>,
    /// Process start (for the startup-time log line).
    launched: Instant,
    /// The file dialog that is open (at most one), and its answer channel.
    dialog: Option<(Pending, std::sync::mpsc::Receiver<Vec<PathBuf>>)>,
}

impl App {
    fn create_ui(&mut self, el: &ActiveEventLoop) -> Result<()> {
        self.ui = Some(UiWin::open(el, &self.gpu, Window::default_attributes().with_title("EVJ").with_maximized(true))?);
        self.open_preview()?;
        let (tx, rx) = channel();
        self.engine.send(Command::ShareTransitionPreview(tx));
        let shared = rx.recv_timeout(Duration::from_secs(5))??;
        if let Some(win) = self.ui.as_mut() {
            self.tr_preview = Some(Preview::open(&self.gpu, &mut win.egui_rend, &shared)?);
        }
        self.open_cue_preview()
    }

    fn open_preview(&mut self) -> Result<()> {
        let Some(win) = self.ui.as_mut() else { return Ok(()) };
        if let Some(old) = self.preview.take() {
            old.release(&mut win.egui_rend);
        }
        let (tx, rx) = channel();
        self.engine.send(Command::SharePreview(tx));
        let shared = rx.recv_timeout(Duration::from_secs(5))??;
        self.preview = Some(Preview::open(&self.gpu, &mut win.egui_rend, &shared)?);
        Ok(())
    }

    fn open_cue_preview(&mut self) -> Result<()> {
        let Some(win) = self.ui.as_mut() else { return Ok(()) };
        if let Some(old) = self.cue_preview.take() {
            old.release(&mut win.egui_rend);
        }
        let (tx, rx) = channel();
        self.engine.send(Command::ShareCuePreview(tx));
        let shared = rx.recv_timeout(Duration::from_secs(5))??;
        self.cue_preview = Some(Preview::open(&self.gpu, &mut win.egui_rend, &shared)?);
        self.state.cue_sent = None; // the engine forgot the cue with its old texture: send it again
        Ok(())
    }

    fn open_tr_preview(&mut self) -> Result<()> {
        let Some(win) = self.ui.as_mut() else { return Ok(()) };
        if let Some(old) = self.tr_preview.take() {
            old.release(&mut win.egui_rend);
        }
        let (tx, rx) = channel();
        self.engine.send(Command::ShareTransitionPreview(tx));
        let shared = rx.recv_timeout(Duration::from_secs(5))??;
        self.tr_preview = Some(Preview::open(&self.gpu, &mut win.egui_rend, &shared)?);
        Ok(())
    }

    /// Opens / closes / redraws the presenter window (30 fps is plenty for it).
    fn presenter(&mut self, el: &ActiveEventLoop, snap: &evj_engine::Snapshot, reopen: bool) -> Result<()> {
        if !self.state.present.show_presenter {
            self.presenter = None;
            return Ok(());
        }
        let p = match self.presenter.as_mut() {
            Some(p) => p,
            None => {
                let attrs = Window::default_attributes().with_title("EVJ Presenter").with_inner_size(winit::dpi::LogicalSize::new(1280.0, 760.0));
                let win = UiWin::open(el, &self.gpu, attrs)?;
                self.presenter.insert(PresenterWin { win, live: None, images: present::SlideImages::start(), last: Instant::now() - Duration::from_secs(1) })
            }
        };
        if reopen || p.live.is_none() {
            // (Re)opens the shared output picture on this window's renderer.
            if let Some(old) = p.live.take() {
                old.release(&mut p.win.egui_rend);
            }
            // The same texture as the main preview (asking the engine again would replace it).
            if let Some(main) = self.preview.as_ref() {
                p.live = Preview::open(&self.gpu, &mut p.win.egui_rend, &main.shared).ok();
            }
        }
        if p.last.elapsed() < Duration::from_millis(33) {
            return Ok(());
        }
        p.last = Instant::now();
        let live = p.live.as_ref().map(|l| (l.tex_id(), l.size));
        let mut act = Actions::default();
        let (state, images) = (&mut self.state, &mut p.images);
        p.win.frame(&self.gpu, &[p.live.as_ref()], |ui| present::presenter(ui, state, snap, live, images, &mut act))?;
        for c in act.commands {
            self.engine.send(c);
        }
        Ok(())
    }

    /// Small pictures of what each output shows, for the Output Manager.
    fn refresh_output_previews(&mut self) {
        if !dock::is_open(&self.state.layout, dock::Panel::Outputs) || self.last_out_preview.elapsed() < Duration::from_millis(500) {
            return;
        }
        self.last_out_preview = Instant::now();
        let Some(win) = self.ui.as_ref() else { return };
        let configs = self.state.project.outputs.clone();
        self.out_previews.resize(configs.len(), None);
        for (i, config) in configs.into_iter().enumerate() {
            let (tx, rx) = channel();
            self.engine.send(Command::RenderOutputPreview { config, width: 160, height: 90, reply: tx });
            let Ok(px) = rx.recv_timeout(Duration::from_millis(200)) else { continue };
            if px.len() == 160 * 90 * 4 {
                let img = egui::ColorImage::from_rgba_unmultiplied([160, 90], &px);
                match &mut self.out_previews[i] {
                    Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                    slot => *slot = Some(win.egui_ctx.load_texture(format!("out{i}"), img, egui::TextureOptions::LINEAR)),
                }
            }
        }
    }

    fn poll_background(&mut self, el: &ActiveEventLoop, snap: &evj_engine::Snapshot) -> Result<()> {
        if snap.device_resets != self.seen_resets {
            self.seen_resets = snap.device_resets;
            self.open_preview()?;
            self.open_tr_preview()?;
            self.open_cue_preview()?;
            self.state.previewing = None;
            self.state.status = "The graphics driver was reset — EVJ recovered and restarted the clips.".into();
        }
        if self.outputs.poll(el, &self.engine, &self.state.project.outputs)? || self.state.display_gpus.is_empty() {
            self.state.status = format!("Monitors: {}", self.outputs.monitors.join(", "));
            self.state.display_gpus = evj_render::display_adapters();
        }
        if self.state.engine_gpu != snap.adapter {
            self.state.engine_gpu = snap.adapter.clone();
        }
        for f in self.jobs.poll() {
            match f {
                jobs::Finished::Ok { input, output } => {
                    let mut n = 0;
                    for p in io::media_paths_mut(&mut self.state.project) {
                        if *p == input {
                            *p = output.clone();
                            n += 1;
                        }
                    }
                    self.state.dirty |= n > 0;
                    self.state.status = format!("Converted to HAP: {} ({n} slot(s) updated)", output.display());
                }
                jobs::Finished::Failed { input, error } => self.state.status = format!("HAP conversion of {} failed: {error}", input.display()),
            }
        }
        for f in self.imports.poll() {
            match f {
                present::Imported::Ok { input, deck } => present::imported(&mut self.state, &input, Ok(deck)),
                present::Imported::Failed { input, error } => present::imported(&mut self.state, &input, Err(error)),
            }
        }
        self.state.jobs = self.jobs.status();
        self.state.jobs.extend(self.imports.status());
        self.state.monitors = self.outputs.monitors.clone();
        self.state.output_open = self.outputs.live;
        self.refresh_output_previews();
        Ok(())
    }

    /// Grid slot under the mouse cursor (drag & drop from Explorer carries no position).
    fn slot_under_cursor(&self) -> Option<(usize, usize)> {
        let win = self.ui.as_ref()?;
        let mut pt = POINT::default();
        unsafe {
            GetCursorPos(&mut pt).ok()?;
            let _ = ScreenToClient(HWND(hwnd(&win.window).ok()? as _), &mut pt);
        }
        let ppp = win.egui_ctx.pixels_per_point();
        let pos = egui::pos2(pt.x as f32 / ppp, pt.y as f32 / ppp);
        self.state.slot_rects.iter().find(|(r, _, _)| r.contains(pos)).map(|&(_, l, c)| (l, c))
    }

    fn set_title(&self) {
        if let Some(w) = &self.ui {
            let name = self.state.project_path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned());
            w.window.set_title(&format!("EVJ — {}{}", name.as_deref().unwrap_or("Untitled"), if self.state.dirty { " *" } else { "" }));
        }
    }

    fn open_project(&mut self, path: &Path, act: &mut Actions) {
        match io::load(path) {
            Ok(mut p) => {
                // Older shows: Layer 1 becomes the top layer, sequences become scene chains.
                let migrated = p.migrate();
                self.remember(path);
                self.state.replace_project(p, Some(path.to_path_buf()), act);
                self.state.missing = io::missing_media(&self.state.project);
                self.state.status = format!("Opened {}", path.display());
                if migrated {
                    self.state.dirty = true;
                    self.state.status = format!("Opened {} — updated for this version (Layer 1 is now the top layer; sequences are scene chains)", path.display());
                }
                // Older shows: the Materi list moves into the grid.
                if self.state.project.migrate_materi() {
                    self.state.dirty = true;
                    self.state.status = format!("Opened {} — its presentations are now in the grid (layer {})", path.display(), self.state.project.presentation_layer() + 1);
                }
            }
            Err(e) => self.state.status = format!("{e:#}"),
        }
    }

    /// Puts `path` on top of the recent shows.
    fn remember(&mut self, path: &Path) {
        if let Some(f) = io::recent_file() {
            self.state.recent = io::add_recent(&f, path);
        }
    }

    /// Opens a file dialog on its own thread (one at a time); the answer goes to `dialog_done`.
    fn ask(&mut self, what: Pending, ask: dialogs::Ask) {
        if self.dialog.is_some() {
            return;
        }
        let owner = self.ui.as_ref().and_then(|w| hwnd(&w.window).ok()).unwrap_or(0);
        self.dialog = Some((what, dialogs::spawn(owner, ask)));
    }

    /// Answer of the open dialog, if it has arrived.
    fn dialog_answer(&mut self) -> Option<(Pending, Vec<PathBuf>)> {
        let r = self.dialog.as_ref().map(|(_, rx)| rx.try_recv())?;
        match r {
            Ok(files) => self.dialog.take().map(|(what, _)| (what, files)),
            Err(std::sync::mpsc::TryRecvError::Empty) => None,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.dialog = None; // the dialog thread failed
                None
            }
        }
    }

    fn dialog_done(&mut self, what: Pending, files: Vec<PathBuf>, act: &mut Actions) {
        if files.is_empty() {
            return; // cancelled
        }
        match what {
            Pending::Open => self.open_project(&files[0], act),
            Pending::AddClips(layer, col) => {
                let todo = self.state.place(layer, col, &files);
                self.start_imports(todo);
            }
            Pending::AttachAudio(deck, layer, col) => timeline::attach(&mut self.state, deck, layer, col, files[0].clone()),
            Pending::Relink => {
                let n = io::relink(&mut self.state.project, &files[0]);
                self.state.missing = io::missing_media(&self.state.project);
                self.state.status = format!("Relinked {n} file(s); {} still missing", self.state.missing.len());
                self.state.dirty |= n > 0;
            }
            Pending::SaveAs { quit } => {
                self.save_to(files[0].clone());
                if quit && !self.state.dirty {
                    act.quit = true;
                }
            }
            Pending::ImportFolder => {
                let media = expand_drops(files);
                let todo = self.state.place(0, 0, &media);
                self.start_imports(todo);
                self.state.status = format!("Added {} file(s) from the folder", media.len());
            }
        }
    }

    /// Starts background imports of PowerPoint / PDF files already placed in the grid.
    fn start_imports(&mut self, files: Vec<PathBuf>) {
        for f in files {
            let c = &self.state.project.composition;
            let r = match present::deck_dir(self.state.project_path.as_deref(), &f) {
                Some(out) => self.imports.start(f.clone(), out, c.width, c.height),
                None => Err("no folder for the imported slides".into()),
            };
            match r {
                Ok(()) => self.state.status = format!("Importing {} — slides, animations and notes (PowerPoint runs hidden)…", f.display()),
                Err(e) => present::imported(&mut self.state, &f, Err(e)),
            }
        }
    }

    /// Where "Import presentation…" puts the file: the selected empty slot, else the first
    /// empty column of the top layer.
    fn presentation_slot(&self) -> (usize, usize) {
        let d = self.state.project.deck();
        if let ui::Selection::Slot(l, c) = self.state.selected {
            if d.is_some_and(|d| d.clip(l, c).is_none()) {
                return (l, c);
            }
        }
        let top = self.state.project.composition.layers.len().saturating_sub(1);
        let col = d.and_then(|d| d.slots.get(top)).map_or(0, |row| row.iter().position(Option::is_none).unwrap_or(row.len()));
        (top, col)
    }

    /// Save to `path`, or ask where first (Save As).
    fn save(&mut self, path: Option<PathBuf>) {
        match path {
            Some(p) => self.save_to(p),
            None => self.save_as(false),
        }
    }

    fn save_as(&mut self, quit: bool) {
        let name = self.state.project_path.as_ref().and_then(|p| p.file_stem()).map_or("Show".into(), |s| s.to_string_lossy().into_owned());
        let (f, e) = dialogs::PROJECT_FILTER;
        let ask = dialogs::Ask::Save { title: "Save show".into(), filter: (f.into(), e.into()), name, ext: io::EXTENSION.into() };
        self.ask(Pending::SaveAs { quit }, ask);
    }

    fn save_to(&mut self, path: PathBuf) {
        match io::save(&self.state.project, &path) {
            Ok(()) => {
                self.remember(&path);
                self.state.project_path = Some(path.clone());
                self.state.dirty = false;
                self.state.autosave_error = None;
                self.state.last_saved = Some(present::local_time());
                self.state.status = format!("Saved {}", path.display());
            }
            Err(e) => self.state.status = format!("Save failed: {e:#}"),
        }
    }

    /// Every menu path goes through here: the lock guard, then the unsaved-changes prompt.
    fn handle_menu(&mut self, m: Menu, act: &mut Actions) {
        if menu_blocked(self.state.locked, &m) {
            self.state.status = "Locked: unlock to change the show".into();
            return;
        }
        if matches!(m, Menu::New | Menu::Open | Menu::OpenRecent(_)) && self.state.dirty {
            self.state.confirm_discard = Some(m);
            return;
        }
        self.run_menu(m, act);
    }

    fn run_menu(&mut self, m: Menu, act: &mut Actions) {
        self.state.confirm_discard = None;
        match m {
            Menu::New => self.state.replace_project(Project::new_default(), None, act),
            Menu::Open => {
                let ask = dialogs::Ask::Open { title: "Open show".into(), filters: dialogs::filters(&[dialogs::PROJECT_FILTER]), multi: false };
                self.ask(Pending::Open, ask);
            }
            Menu::Save => self.save(self.state.project_path.clone()),
            Menu::SaveAs => self.save(None),
            Menu::AddClips(layer, col) => {
                let ask = dialogs::Ask::Open { title: "Add clips".into(), filters: dialogs::filters(&[dialogs::MEDIA_FILTER, ("All files", "*.*")]), multi: true };
                self.ask(Pending::AddClips(layer, col), ask);
            }
            Menu::Relink => self.ask(Pending::Relink, dialogs::Ask::Folder { title: "Folder with the media".into() }),
            Menu::AttachAudio(layer, col) => {
                let filter = ("Audio", "*.mp3;*.wav;*.m4a;*.aac;*.flac;*.wma;*.ogg");
                let ask = dialogs::Ask::Open { title: "Attach audio".into(), filters: dialogs::filters(&[filter]), multi: false };
                self.ask(Pending::AttachAudio(self.state.project.active_deck, layer, col), ask);
            }
            Menu::ImportFolder => self.ask(Pending::ImportFolder, dialogs::Ask::Folder { title: "Media folder".into() }),
            Menu::OpenRecent(p) => {
                if p.exists() {
                    self.open_project(&p, act);
                } else {
                    self.state.status = format!("{} is gone (moved or deleted)", p.display());
                }
            }
            Menu::ImportPresentation => {
                let (layer, col) = self.presentation_slot();
                let filter = ("Presentations", "*.pptx;*.ppt;*.pptm;*.ppsx;*.pps;*.odp;*.pdf;deck.json");
                let ask = dialogs::Ask::Open { title: "Import presentations".into(), filters: dialogs::filters(&[filter]), multi: true };
                self.ask(Pending::AddClips(layer, col), ask);
            }
            Menu::Recover(yes) => {
                if let Some(p) = self.state.recovery.take() {
                    if yes {
                        self.open_project(&p, act);
                        self.state.project_path = None;
                        self.state.dirty = true;
                    }
                }
            }
        }
    }

    fn autosave(&mut self) {
        if !self.state.dirty || self.last_autosave.elapsed() < AUTOSAVE_EVERY {
            return;
        }
        self.last_autosave = Instant::now();
        if let Some(path) = autosave_target(self.state.project_path.as_deref(), self.state.dirty) {
            match io::save(&self.state.project, &path) {
                Ok(()) => {
                    self.state.dirty = false;
                    self.state.autosave_error = None;
                    self.state.last_saved = Some(present::local_time());
                }
                // Stays dirty: retried on the next tick; the recovery copy below is still written.
                Err(e) => self.state.autosave_error = Some(format!("{e:#}")),
            }
        }
        let (project, path) = (self.state.project.clone(), io::autosave_path());
        let _ = std::thread::Builder::new().name("evj-autosave".into()).spawn(move || {
            if let Some(path) = path {
                let _ = io::save(&project, &path);
            }
        });
    }

    fn redraw_ui(&mut self, el: &ActiveEventLoop) -> Result<()> {
        if !self.drops.is_empty() && self.state.locked {
            self.drops.clear();
            self.state.drop_target = None;
            self.state.status = "Locked: unlock to add media".into();
        }
        if !self.drops.is_empty() {
            let files = expand_drops(std::mem::take(&mut self.drops));
            let (layer, col) = self.state.drop_target.take().or_else(|| self.slot_under_cursor()).unwrap_or((0, 0));
            let todo = self.state.place(layer, col, &files);
            self.start_imports(todo);
            self.state.status = format!("Added {} clip(s) to layer {}", files.len(), layer + 1);
        }
        let snap = self.engine.snapshot();
        let reset = snap.device_resets != self.seen_resets;
        self.poll_background(el, &snap)?;
        let Some(win) = self.ui.as_mut() else { return Ok(()) };
        self.thumbs.poll(&win.egui_ctx);
        self.waves.poll();
        let preview = self.preview.as_ref().map(|p| (p.tex_id(), p.size));
        let tr_preview = self.tr_preview.as_ref().map(|p| (p.tex_id(), p.size));
        let cue_preview = self.cue_preview.as_ref().map(|p| (p.tex_id(), p.size));
        let mut act = Actions::default();
        let (state, thumbs, waves, out_previews) = (&mut self.state, &mut self.thumbs, &mut self.waves, &self.out_previews);
        win.frame(&self.gpu, &[self.preview.as_ref(), self.tr_preview.as_ref(), self.cue_preview.as_ref()], |ui| {
            act.absorb(ui::draw(ui, state, &snap, preview, tr_preview, cue_preview, thumbs, waves, out_previews))
        })?;
        if self.soak.as_mut().is_some_and(|s| s.tick(&mut self.state, &snap, &mut act)) {
            act.quit = true;
        }

        if let Some(m) = act.menu.take() {
            self.handle_menu(m, &mut act);
        }
        if let Some((what, files)) = self.dialog_answer() {
            self.dialog_done(what, files, &mut act);
        }
        // Everything that can change the show runs before the engine commands are sent
        // (replace_project queues sync commands + a preview re-open).
        for a in std::mem::take(&mut act.app) {
            match a {
                shortcuts::AppAction::New => self.handle_menu(Menu::New, &mut act),
                shortcuts::AppAction::Open => self.handle_menu(Menu::Open, &mut act),
                shortcuts::AppAction::Save => self.save(self.state.project_path.clone()),
                shortcuts::AppAction::SaveAs => self.save(None),
                _ => {}
            }
        }
        if let Some(save_first) = act.discard_answer.take() {
            if let Some(m) = self.state.confirm_discard.take() {
                match (save_first, self.state.project_path.clone()) {
                    (true, Some(p)) => {
                        self.save_to(p);
                        if self.state.dirty {
                            self.state.confirm_discard = Some(m); // save failed: ask again
                        } else {
                            self.run_menu(m, &mut act);
                        }
                    }
                    (true, None) => self.save_as(false), // the user repeats New / Open after saving
                    (false, _) => self.run_menu(m, &mut act),
                }
            }
        }
        if act.save_and_quit {
            match self.state.project_path.clone() {
                Some(p) => {
                    self.save_to(p);
                    act.quit = !self.state.dirty;
                }
                None => self.save_as(true),
            }
        }
        for c in act.commands.drain(..) {
            self.engine.send(c);
        }
        if act.reopen_preview {
            self.engine.sync();
            self.open_preview()?;
            self.open_cue_preview()?;
        }
        self.presenter(el, &snap, reset || act.reopen_preview)?;
        if act.toggle_output {
            if self.outputs.live {
                self.outputs.stop(&self.engine);
            } else {
                self.outputs.go_live(el, &self.engine, &self.state.project.outputs)?;
            }
        }
        if act.outputs_changed {
            self.outputs.sync(el, &self.engine, &self.state.project.outputs)?;
        }
        if act.identify {
            self.outputs.identify(el, &self.engine, &self.state.project.outputs)?;
        }
        if let Some((path, variant)) = act.convert.take() {
            let duration = self.thumbs.info(&path).map_or(0.0, |i| i.duration);
            self.state.status = match self.jobs.start(path.clone(), variant, duration) {
                Ok(()) => format!("Converting {} to HAP…", path.display()),
                Err(e) => format!("Cannot convert: {e}"),
            };
        }
        let now = self.launched.elapsed().as_secs_f64();
        if self.layout_gate.rested(now) && self.layout_gate.due(&dock::to_json(&self.state.layout), now) {
            if let Some(f) = shortcuts::settings_file() {
                dock::save(&f, &self.state.layout);
            }
        }
        if std::mem::take(&mut self.state.audio_dirty) {
            if let Some(f) = shortcuts::settings_file() {
                audioset::save(&f, &self.state.audio);
            }
        }
        if let Some(src) = act.retry_import.take() {
            self.start_imports(vec![src]);
        }
        if act.quit {
            if let Some(f) = shortcuts::settings_file() {
                dock::save(&f, &self.state.layout);
            }
            self.imports.cancel_all();
            self.outputs.stop(&self.engine);
            el.exit();
        }
        self.set_title();
        self.autosave();
        self.bench(el)
    }

    fn bench(&mut self, el: &ActiveEventLoop) -> Result<()> {
        let Some(secs) = self.args.bench else { return Ok(()) };
        let t = self.started.elapsed().as_secs_f64();
        if !self.bench_reset && t > WARMUP {
            self.engine.send(Command::ResetStats);
            self.bench_reset = true;
            self.bench_cpu = Some((soak::process_cpu_time(), Instant::now()));
            if let Some(w) = self.ui.as_mut() {
                w.times.reset();
            }
        }
        if t > secs + WARMUP {
            let s = self.engine.snapshot();
            // CPU % of the whole machine (all cores), like Task Manager shows it.
            let cpu = self.bench_cpu.map_or(0.0, |(c0, t0)| {
                let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
                (soak::process_cpu_time() - c0).as_secs_f64() / (t0.elapsed().as_secs_f64() * cores) * 100.0
            });
            let (private, _) = soak::process_memory_mb();
            let mut r = format!(
                "EVJ bench (engine)\nadapter: {}\nfps: {:.1}\np99: {:.2} ms\ndropped: {}\noutputs: {}\ncpu: {cpu:.1} %\ngpu frame: {:.2} ms\ngpu memory: {:.0} MB\nprivate memory: {private:.0} MB\naudio underruns: {}\n",
                s.adapter, s.fps, s.p99_ms, s.dropped, s.outputs, s.gpu_ms, s.gpu_memory_mb, s.audio_underruns
            );
            for (i, l) in s.layers.iter().enumerate() {
                r += &format!("layer {}: {} [{:?}] {}\n", i + 1, l.clip_name.as_deref().unwrap_or("-"), l.kind, l.error.as_deref().unwrap_or(""));
            }
            for (name, ms) in &s.gpu_stages {
                r += &format!("gpu {name}: {ms:.2} ms\n");
            }
            if let Some(w) = &self.ui {
                r += &w.times.report();
            }
            println!("{r}");
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
            std::fs::write(format!("evj-{stamp}.bench.txt"), r)?;
            el.exit();
        }
        Ok(())
    }

    fn fail(&mut self, el: &ActiveEventLoop, e: anyhow::Error) {
        self.error = Some(e);
        el.exit();
    }

    fn start(&mut self, el: &ActiveEventLoop) -> Result<()> {
        self.create_ui(el)?;
        evj_core::log::info("app", &format!("ready in {} ms", self.launched.elapsed().as_millis()));
        // Raw keyboard / mouse even without focus: presenter clickers and laser pointers.
        el.listen_device_events(DeviceEvents::Always);
        self.state.recent = io::recent_file().map(|f| io::load_recent(&f)).unwrap_or_default();
        self.state.shortcuts = shortcuts::settings_file().map(|f| shortcuts::Shortcuts::load(&f)).unwrap_or_else(shortcuts::Shortcuts::defaults);
        // The dock layout and the sound outputs of this laptop.
        self.state.layout = shortcuts::settings_file().map(|f| dock::load(&f)).unwrap_or_else(dock::preset_live);
        self.state.audio = shortcuts::settings_file().map(|f| audioset::load(&f)).unwrap_or_default();
        for c in audioset::commands(&self.state.audio) {
            self.engine.send(c);
        }
        let mut act = Actions::default();
        let files = std::mem::take(&mut self.args.files);
        match files.first() {
            Some(p) if p.extension().is_some_and(|e| e == io::EXTENSION) => {
                self.open_project(p, &mut act);
                if self.args.bench.is_some() {
                    // The bench scenario: column 1 on Program, layer 1 of column 2 on Preview.
                    self.state.trigger_column(0, &mut act);
                    if self.state.project.deck().and_then(|d| d.clip(0, 1)).is_some() {
                        self.state.selected = ui::Selection::Slot(0, 1);
                    }
                }
            }
            Some(_) => {
                // Clips on the command line: one per layer, all playing.
                while self.state.project.composition.layers.len() < files.len() {
                    self.state.project.add_layer();
                }
                act.commands.extend(self.state.sync_engine());
                act.reopen_preview = true;
                for (i, f) in files.iter().enumerate() {
                    self.state.place(i, 0, std::slice::from_ref(f));
                    self.state.trigger(i, 0, &mut act);
                }
                self.state.dirty = false;
            }
            None => {
                if let Some(a) = io::autosave_path().filter(|p| p.exists()) {
                    self.state.recovery = Some(a);
                }
                // A normal start (no show, no bench): the welcome screen.
                if self.args.bench.is_none() && self.args.soak.is_none() {
                    self.state.open_welcome();
                }
            }
        }
        for c in act.commands {
            self.engine.send(c);
        }
        if act.reopen_preview {
            self.engine.sync();
            self.open_preview()?;
        }
        if let Some(i) = self.args.output_monitor {
            let name = el.available_monitors().nth(i).and_then(|m| m.name());
            if let Some(o) = self.state.project.outputs.first_mut() {
                o.monitor = name;
            }
        }
        if let Some(hours) = self.args.soak {
            self.soak = Some(soak::Soak::new(hours));
        }
        if self.args.bench.is_some() || self.args.soak.is_some() {
            self.outputs.go_live(el, &self.engine, &self.state.project.outputs)?;
        }
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.ui.is_none() {
            if let Err(e) = self.start(el) {
                self.fail(el, e);
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.outputs.handle(&self.engine, id, &event) {
            return;
        }
        if let WindowEvent::Focused(f) = event {
            self.focused = f;
        }
        if let Some(p) = self.presenter.as_mut().filter(|p| p.win.window.id() == id) {
            let _ = p.win.egui_winit.on_window_event(&p.win.window, &event);
            match event {
                WindowEvent::CloseRequested => self.state.present.show_presenter = false,
                WindowEvent::Resized(s) => {
                    if let Err(e) = p.win.chain.resize(&self.gpu, s.width, s.height) {
                        self.fail(el, e);
                    }
                }
                _ => {}
            }
            return;
        }
        let Some(win) = self.ui.as_mut() else { return };
        let _ = win.egui_winit.on_window_event(&win.window, &event);
        match event {
            WindowEvent::CloseRequested => {
                if self.outputs.live || self.state.dirty {
                    self.state.confirm_quit = true;
                } else {
                    el.exit();
                }
            }
            WindowEvent::Resized(s) => {
                if let Err(e) = win.chain.resize(&self.gpu, s.width, s.height) {
                    self.fail(el, e);
                }
            }
            WindowEvent::HoveredFile(_) => self.state.drop_target = self.slot_under_cursor(),
            WindowEvent::HoveredFileCancelled => self.state.drop_target = None,
            WindowEvent::DroppedFile(p) => {
                if self.drops.is_empty() {
                    self.state.drop_target = self.slot_under_cursor().or(self.state.drop_target);
                }
                self.drops.push(p);
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.redraw_ui(el) {
                    self.fail(el, e);
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        match event {
            DeviceEvent::Key(RawKeyEvent { physical_key: PhysicalKey::Code(code), state }) => {
                let fresh = match state {
                    ElementState::Pressed => self.keys_down.insert(code),
                    ElementState::Released => {
                        self.keys_down.remove(&code);
                        false
                    }
                };
                let p = &self.state.present;
                if !fresh || !p.active() || (!self.focused && !p.background_clicker) {
                    return;
                }
                // Typing in a text field is not a click.
                if self.focused && self.ui.as_ref().is_some_and(|w| w.egui_ctx.egui_wants_keyboard_input()) {
                    return;
                }
                if let Some(k) = present::clicker_key(code, self.focused) {
                    self.state.present.keys.push(k);
                }
            }
            DeviceEvent::MouseMotion { delta: (dx, dy) } => self.state.present.mouse_delta(dx, dy),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(w) = &self.ui {
            w.window.request_redraw();
        }
    }
}

/// Started from a terminal (`evj --bench 10`): print there. Started from the Start menu / a
/// shortcut there is no parent console and nothing happens.
fn attach_console() {
    use windows::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};
    // SAFETY: plain Win32 call without pointers; failing (no parent console) is fine.
    let _ = unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

fn main() -> std::process::ExitCode {
    attach_console();
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            // No console in release builds: say it in a message box (and the log), not only on stderr.
            evj_core::log::error("app", &format!("could not run: {e:#}"));
            eprintln!("EVJ: {e:#}");
            fatal_box(&format!("{e:#}"));
            std::process::ExitCode::FAILURE
        }
    }
}

fn fatal_box(text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    use windows::core::HSTRING;
    // SAFETY: both strings live across the (modal) call.
    unsafe { MessageBoxW(None, &HSTRING::from(format!("EVJ could not start or stopped:

{text}")), &HSTRING::from("EVJ"), MB_OK | MB_ICONERROR) };
}

fn run() -> Result<()> {
    let launched = Instant::now();
    let args = parse_args()?;
    if let Some(dir) = io::app_dir() {
        evj_core::log::init(&dir);
        crash::install(&dir.join("crash"));
    }
    evj_core::log::info("app", &format!("EVJ {} starting", env!("CARGO_PKG_VERSION")));
    let project = Project::new_default();
    let engine = Engine::start(EngineConfig {
        device: DeviceKind::Hardware,
        manual_clock: false,
        width: project.composition.width,
        height: project.composition.height,
        layers: project.composition.layers.len(),
        effect_folders: effect_folders(),
        audio: true,
    })?;
    // Separate device for the UI: nothing it does can stall the engine's GPU context.
    let gpu = Gpu::new(DeviceKind::Hardware)?;
    // The audience's picture first: the operator's window waits when the GPU is busy.
    if let Err(e) = gpu.set_priority(-7) {
        evj_core::log::warn("app", &format!("UI GPU priority: {e:#}"));
    }
    let mut app = App {
        args,
        gpu,
        engine,
        state: UiState::new(project),
        thumbs: Thumbnailer::start(),
        layout_gate: dock::SaveGate::default(),
        waves: waveform::Waveforms::start(),
        ui: None,
        preview: None,
        tr_preview: None,
        cue_preview: None,
        started: Instant::now(),
        bench_reset: false,
        bench_cpu: None,
        outputs: outputs::Outputs::new(),
        jobs: jobs::Jobs::new(),
        out_previews: Vec::new(),
        last_out_preview: Instant::now(),
        seen_resets: 0,
        drops: Vec::new(),
        last_autosave: Instant::now(),
        error: None,
        presenter: None,
        imports: present::Imports::new(),
        keys_down: HashSet::new(),
        focused: true,
        soak: None,
        launched,
        dialog: None,
    };
    let el = EventLoop::new()?;
    el.set_control_flow(ControlFlow::Poll);
    el.run_app(&mut app)?;
    app.outputs.stop(&app.engine);
    // Clean exit: nothing to recover next time.
    if let Some(a) = io::autosave_path() {
        let _ = std::fs::remove_file(a);
    }
    match app.error.take() {
        Some(e) => {
            evj_core::log::error("app", &format!("stopped: {e:#}"));
            Err(e)
        }
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autosave_target_only_for_saved_shows() {
        assert_eq!(autosave_target(None, true), None, "untitled: recovery copy only");
        assert_eq!(autosave_target(Some(Path::new("C:/show.vjproj")), false), None, "nothing changed");
        assert_eq!(autosave_target(Some(Path::new("C:/show.vjproj")), true), Some(PathBuf::from("C:/show.vjproj")));
    }
}

#[cfg(test)]
mod lock_tests {
    use super::*;

    #[test]
    fn lock_blocks_show_changing_menus() {
        assert!(menu_blocked(true, &Menu::OpenRecent(PathBuf::from("a.vjproj"))));
        assert!(menu_blocked(true, &Menu::ImportFolder));
        assert!(menu_blocked(true, &Menu::AttachAudio(0, 0)));
        assert!(menu_blocked(true, &Menu::New));
        assert!(!menu_blocked(true, &Menu::Save), "saving is always allowed");
        assert!(!menu_blocked(false, &Menu::New));
    }
}
