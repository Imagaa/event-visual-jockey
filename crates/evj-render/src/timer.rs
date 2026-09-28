//! GPU time of a frame (D3D11 timestamp queries), collected a few frames later so the CPU never waits.
//! Optional marks split the frame into stages (e.g. per layer, outputs, preview).
use crate::Gpu;
use anyhow::{Context, Result};
use windows::Win32::Foundation::S_OK;
use windows::Win32::Graphics::Direct3D11::*;
use windows::core::Interface;

const SLOTS: usize = 4;
/// Marks per frame (begin + stages + end).
const MARKS: usize = 16;

struct Slot {
    disjoint: ID3D11Query,
    marks: Vec<ID3D11Query>,
    used: usize,
    pending: bool,
}

pub struct GpuTimer {
    slots: Vec<Slot>,
    next: usize,
    /// GPU milliseconds of the most recent frame that finished measuring.
    pub last_ms: Option<f32>,
    /// Milliseconds from the frame start to each mark of that frame.
    pub last_marks: Vec<f32>,
}

fn query(gpu: &Gpu, kind: D3D11_QUERY) -> Result<ID3D11Query> {
    let mut q = None;
    unsafe { gpu.device.CreateQuery(&D3D11_QUERY_DESC { Query: kind, MiscFlags: 0 }, Some(&mut q))? };
    q.context("query")
}

/// Non-blocking read; None while the GPU has not got there yet (S_FALSE).
fn read<T: Default>(ctx: &ID3D11DeviceContext, q: &ID3D11Query) -> Option<T> {
    let mut v = T::default();
    let hr = unsafe {
        (Interface::vtable(ctx).GetData)(Interface::as_raw(ctx), q.as_raw(), (&mut v as *mut T).cast(), size_of::<T>() as u32, D3D11_ASYNC_GETDATA_DONOTFLUSH.0 as u32)
    };
    (hr == S_OK).then_some(v)
}

impl GpuTimer {
    pub fn new(gpu: &Gpu) -> Result<GpuTimer> {
        let slots = (0..SLOTS)
            .map(|_| {
                let marks = (0..MARKS).map(|_| query(gpu, D3D11_QUERY_TIMESTAMP)).collect::<Result<Vec<_>>>()?;
                Ok(Slot { disjoint: query(gpu, D3D11_QUERY_TIMESTAMP_DISJOINT)?, marks, used: 0, pending: false })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(GpuTimer { slots, next: 0, last_ms: None, last_marks: Vec::new() })
    }

    /// Marks the start of the GPU work to measure. A slot still in flight is skipped (no stall).
    pub fn begin(&mut self, ctx: &ID3D11DeviceContext) -> bool {
        self.collect(ctx);
        let s = &mut self.slots[self.next];
        if s.pending {
            return false;
        }
        s.used = 0;
        unsafe { ctx.Begin(&s.disjoint) };
        self.mark(ctx);
        true
    }

    /// A stage boundary inside the measured frame (ignored past the mark limit).
    pub fn mark(&mut self, ctx: &ID3D11DeviceContext) {
        let s = &mut self.slots[self.next];
        if s.used < MARKS - 1 {
            unsafe { ctx.End(&s.marks[s.used]) };
            s.used += 1;
        }
    }

    pub fn end(&mut self, ctx: &ID3D11DeviceContext) {
        let s = &mut self.slots[self.next];
        unsafe { ctx.End(&s.marks[s.used]) };
        s.used += 1;
        unsafe { ctx.End(&s.disjoint) };
        s.pending = true;
        self.next = (self.next + 1) % SLOTS;
    }

    fn collect(&mut self, ctx: &ID3D11DeviceContext) {
        for i in 0..SLOTS {
            let s = &mut self.slots[(self.next + i) % SLOTS];
            if !s.pending {
                continue;
            }
            let Some(d) = read::<D3D11_QUERY_DATA_TIMESTAMP_DISJOINT>(ctx, &s.disjoint) else { continue };
            let Some(ticks) = s.marks[..s.used].iter().map(|q| read::<u64>(ctx, q)).collect::<Option<Vec<u64>>>() else { continue };
            s.pending = false;
            if d.Disjoint.as_bool() || d.Frequency == 0 || ticks.windows(2).any(|w| w[1] < w[0]) {
                continue;
            }
            let ms = |t: u64| ((t - ticks[0]) as f64 * 1000.0 / d.Frequency as f64) as f32;
            self.last_marks = ticks[1..].iter().map(|&t| ms(t)).collect();
            self.last_ms = self.last_marks.last().copied();
        }
    }
}
