//! Output windows: one per configured output, fullscreen on its monitor (or a window),
//! re-created when monitors come and go.
use anyhow::Result;
use evj_core::output::OutputConfig;
use evj_engine::{Command, Engine};
use std::time::{Duration, Instant};
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Fullscreen, Window, WindowId};

pub struct OutWin {
    pub window: Window,
    pub id: u32,
    /// Index into the project's outputs.
    pub index: usize,
    pub monitor: Option<String>,
}

pub struct Outputs {
    pub wins: Vec<OutWin>,
    pub live: bool,
    pub monitors: Vec<String>,
    next_id: u32,
    last_poll: Instant,
    identify_until: Option<Instant>,
}

impl Outputs {
    pub fn new() -> Outputs {
        Outputs { wins: Vec::new(), live: false, monitors: Vec::new(), next_id: 1, last_poll: Instant::now() - Duration::from_secs(10), identify_until: None }
    }

    fn open(&mut self, el: &ActiveEventLoop, engine: &Engine, index: usize, config: &OutputConfig) -> Result<()> {
        let monitor = config.monitor.as_ref().and_then(|name| el.available_monitors().find(|m| m.name().as_deref() == Some(name.as_str())));
        if config.monitor.is_some() && monitor.is_none() {
            return Ok(()); // unplugged: opens when it comes back
        }
        let mut attrs = Window::default_attributes().with_title(format!("EVJ Output — {}", config.name)).with_window_icon(crate::window_icon());
        attrs = match monitor {
            Some(m) => attrs.with_fullscreen(Some(Fullscreen::Borderless(Some(m)))),
            None => attrs.with_inner_size(PhysicalSize::new(960, 540)),
        };
        let window = el.create_window(attrs)?;
        let size = window.inner_size();
        let id = self.next_id;
        self.next_id += 1;
        engine.send(Command::OpenOutput { id, hwnd: crate::hwnd(&window)?, width: size.width, height: size.height });
        engine.send(Command::SetOutputConfig { id, config: config.clone() });
        self.wins.push(OutWin { window, id, index, monitor: config.monitor.clone() });
        Ok(())
    }

    fn close_where(&mut self, engine: &Engine, drop: impl Fn(&OutWin) -> bool) {
        let (gone, keep): (Vec<OutWin>, Vec<OutWin>) = self.wins.drain(..).partition(drop);
        self.wins = keep;
        if gone.is_empty() {
            return;
        }
        for w in &gone {
            engine.send(Command::CloseOutput { id: w.id });
        }
        engine.sync(); // swapchains released before their windows go
    }

    pub fn go_live(&mut self, el: &ActiveEventLoop, engine: &Engine, configs: &[OutputConfig]) -> Result<()> {
        self.live = true;
        self.sync(el, engine, configs)
    }

    pub fn stop(&mut self, engine: &Engine) {
        self.live = false;
        self.close_where(engine, |_| true);
    }

    /// Makes the open windows match `configs` (monitor changes re-open, settings are pushed to the engine).
    pub fn sync(&mut self, el: &ActiveEventLoop, engine: &Engine, configs: &[OutputConfig]) -> Result<()> {
        if !self.live {
            return Ok(());
        }
        self.close_where(engine, |w| configs.get(w.index).is_none_or(|c| c.monitor != w.monitor));
        let identify = self.identify_until.is_some_and(|t| Instant::now() < t);
        for (i, c) in configs.iter().enumerate() {
            match self.wins.iter().find(|w| w.index == i) {
                Some(w) => {
                    let config = OutputConfig { test_pattern: c.test_pattern || identify, ..c.clone() };
                    engine.send(Command::SetOutputConfig { id: w.id, config });
                }
                None => self.open(el, engine, i, c)?,
            }
        }
        Ok(())
    }

    /// Every second: notices unplugged / re-plugged monitors and the end of "identify".
    pub fn poll(&mut self, el: &ActiveEventLoop, engine: &Engine, configs: &[OutputConfig]) -> Result<bool> {
        if self.last_poll.elapsed() < Duration::from_secs(1) {
            return Ok(false);
        }
        self.last_poll = Instant::now();
        let now: Vec<String> = el.available_monitors().filter_map(|m| m.name()).collect();
        let identify_over = self.identify_until.is_some_and(|t| Instant::now() >= t);
        if identify_over {
            self.identify_until = None;
        }
        let changed = now != self.monitors;
        self.monitors = now;
        if changed {
            let present = self.monitors.clone();
            self.close_where(engine, |w| w.monitor.as_ref().is_some_and(|m| !present.contains(m)));
        }
        if changed || identify_over {
            self.sync(el, engine, configs)?;
        }
        Ok(changed)
    }

    pub fn identify(&mut self, el: &ActiveEventLoop, engine: &Engine, configs: &[OutputConfig]) -> Result<()> {
        self.identify_until = Some(Instant::now() + Duration::from_secs(5));
        self.sync(el, engine, configs)
    }

    /// Handles events of output windows; false when `id` is not one of them.
    pub fn handle(&mut self, engine: &Engine, id: WindowId, event: &WindowEvent) -> bool {
        let Some(w) = self.wins.iter().find(|w| w.window.id() == id) else { return false };
        match event {
            WindowEvent::Resized(s) => engine.send(Command::ResizeOutput { id: w.id, width: s.width, height: s.height }),
            WindowEvent::CloseRequested => {
                let out_id = w.id;
                self.close_where(engine, |w| w.id == out_id);
            }
            _ => {}
        }
        true
    }
}
