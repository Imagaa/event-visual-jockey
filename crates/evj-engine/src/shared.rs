//! Pictures the UI shows (Program / Preview monitors, the transition card): three shared
//! targets written in turn. The UI draws the one written a frame ago — its GPU work is done — so
//! it never waits for the engine's GPU (on a busy iGPU that cross-device wait cost the UI ~10 ms
//! every frame and made the monitors stutter).
use anyhow::{Context, Result};
use evj_render::{Gpu, RenderTarget};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use windows::Win32::Foundation::S_OK;
use windows::Win32::Graphics::Dxgi::IDXGIKeyedMutex;
use windows::core::Interface;

pub const SLOTS: usize = 3;

/// What another device opens: a share handle per slot (keyed mutex, key 0) and the slot to draw.
#[derive(Clone, Debug)]
pub struct SharedPreview {
    pub handles: Vec<isize>,
    pub ready: Arc<AtomicUsize>,
    pub width: u32,
    pub height: u32,
}

pub(crate) struct Slot {
    pub rt: RenderTarget,
    pub mutex: IDXGIKeyedMutex,
}

impl Slot {
    /// Never waits: false when the UI holds it.
    pub fn acquire(&self) -> bool {
        unsafe { (Interface::vtable(&self.mutex).AcquireSync)(Interface::as_raw(&self.mutex), 0, 0) == S_OK }
    }

    pub fn release(&self) {
        unsafe {
            let _ = self.mutex.ReleaseSync(0);
        }
    }
}

pub(crate) struct SharedRing {
    slots: Vec<Slot>,
    /// Written this frame (becomes `ready` at the next `tick`).
    written: Option<usize>,
    ready: Arc<AtomicUsize>,
}

impl SharedRing {
    pub fn new(gpu: &Gpu, w: u32, h: u32) -> Result<(SharedRing, SharedPreview)> {
        let mut slots = Vec::new();
        let mut handles = Vec::new();
        for _ in 0..SLOTS {
            let (rt, handle) = RenderTarget::new_shared(gpu, w, h)?;
            let mutex = rt.tex.cast::<IDXGIKeyedMutex>().context("keyed mutex")?;
            slots.push(Slot { rt, mutex });
            handles.push(handle);
        }
        let ready = Arc::new(AtomicUsize::new(0));
        Ok((SharedRing { slots, written: None, ready: ready.clone() }, SharedPreview { handles, ready, width: w, height: h }))
    }

    pub fn size(&self) -> (u32, u32) {
        (self.slots[0].rt.width, self.slots[0].rt.height)
    }

    /// Start of an engine frame: the picture written last frame is finished — the UI may show it.
    pub fn tick(&mut self) {
        if let Some(w) = self.written.take() {
            self.ready.store(w, Ordering::Release);
        }
    }

    /// Draws into a slot the UI is not showing; skipped this frame when none is free.
    pub fn write(&mut self, draw: impl FnOnce(&RenderTarget)) {
        let ready = self.ready.load(Ordering::Acquire);
        let slots = &self.slots;
        if let Some(i) = next_slot(slots.len(), ready, |i| slots[i].acquire()) {
            draw(&slots[i].rt);
            slots[i].release();
            self.written = Some(i);
        }
    }

    /// The newest picture (this frame's if one was written).
    pub fn latest(&self) -> &Slot {
        &self.slots[self.written.unwrap_or_else(|| self.ready.load(Ordering::Acquire))]
    }
}

/// The slot to write this frame: the first after `ready` that is free (the UI may be holding one).
/// `free` acquires the slot it says yes to.
pub(crate) fn next_slot(n: usize, ready: usize, mut free: impl FnMut(usize) -> bool) -> Option<usize> {
    (1..n).map(|k| (ready + k) % n).find(|&i| free(i))
}

#[cfg(test)]
mod tests {
    use super::next_slot;

    #[test]
    fn the_slot_the_ui_shows_is_never_written() {
        assert_eq!(next_slot(3, 0, |_| true), Some(1));
        assert_eq!(next_slot(3, 2, |_| true), Some(0), "wraps around");
        assert_eq!(next_slot(3, 0, |i| i != 1), Some(2), "the UI holds 1");
        assert_eq!(next_slot(3, 0, |i| i == 0), None, "only the ready one is free: skip this frame");
    }
}
