//! `--soak HOURS`: a random show for hours — clips, columns, transitions, slides, pointer, BPM —
//! logging memory and frame statistics every minute to `%APPDATA%\EVJ\soak-<time>.csv`.
use crate::present::{Key, PointerMode};
use crate::ui::{Actions, UiState};
use evj_engine::{Command, Snapshot};
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};

pub struct Soak {
    started: Instant,
    until: Instant,
    next_action: Instant,
    next_log: Instant,
    rng: u64,
    log: Option<PathBuf>,
    actions: u64,
    /// Private memory (MB) per logged minute.
    memory: Vec<f64>,
}

/// (private bytes, working set) of this process in MB.
pub fn process_memory_mb() -> (f64, f64) {
    use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX};
    use windows::Win32::System::Threading::GetCurrentProcess;
    let mut c = PROCESS_MEMORY_COUNTERS_EX { cb: size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32, ..Default::default() };
    let ok = unsafe { GetProcessMemoryInfo(GetCurrentProcess(), &mut c as *mut _ as *mut PROCESS_MEMORY_COUNTERS, c.cb) };
    if ok.is_err() {
        return (0.0, 0.0);
    }
    (c.PrivateUsage as f64 / 1048576.0, c.WorkingSetSize as f64 / 1048576.0)
}

/// User + kernel CPU time this process has used.
pub fn process_cpu_time() -> Duration {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let (mut a, mut b, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
    if unsafe { GetProcessTimes(GetCurrentProcess(), &mut a, &mut b, &mut kernel, &mut user) }.is_err() {
        return Duration::ZERO;
    }
    let ticks = |f: FILETIME| ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64; // 100 ns
    Duration::from_nanos((ticks(kernel) + ticks(user)) * 100)
}

/// Average of the first and the last `n` samples: the leak check.
pub fn growth(samples: &[f64], n: usize) -> Option<(f64, f64)> {
    if samples.len() < 2 * n || n == 0 {
        return None;
    }
    let avg = |s: &[f64]| s.iter().sum::<f64>() / s.len() as f64;
    Some((avg(&samples[..n]), avg(&samples[samples.len() - n..])))
}

impl Soak {
    pub fn new(hours: f64) -> Soak {
        let now = Instant::now();
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let log = evj_core::io::app_dir().map(|d| d.join(format!("soak-{stamp}.csv")));
        if let Some(l) = &log {
            let _ = std::fs::write(l, "minute,private_mb,working_set_mb,gpu_mb,fps,p99_ms,dropped,device_resets,actions,layer_errors\n");
        }
        Soak {
            started: now,
            until: now + Duration::from_secs_f64(hours * 3600.0),
            next_action: now + Duration::from_secs(5),
            next_log: now + Duration::from_secs(60),
            rng: stamp | 1,
            log,
            actions: 0,
            memory: Vec::new(),
        }
    }

    fn rand(&mut self, n: usize) -> usize {
        // xorshift64
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 7;
        self.rng ^= self.rng << 17;
        (self.rng % n.max(1) as u64) as usize
    }

    fn act(&mut self, st: &mut UiState, act: &mut Actions) {
        let layers = st.project.composition.layers.len();
        let names: Vec<String> = st.project.transitions.iter().map(|t| t.name.clone()).collect();
        st.next_transition = match self.rand(4) {
            0 => None,
            _ => names.get(self.rand(names.len())).cloned(),
        };
        let slots: Vec<(usize, usize)> = st
            .project
            .deck()
            .map(|d| d.slots.iter().enumerate().flat_map(|(l, row)| row.iter().enumerate().filter(|(_, c)| c.is_some()).map(move |(c, _)| (l, c))).collect())
            .unwrap_or_default();
        match self.rand(100) {
            0..55 if !slots.is_empty() => {
                let (l, c) = slots[self.rand(slots.len())];
                st.trigger(l, c, act);
            }
            55..65 => {
                let l = self.rand(layers);
                st.clear(l, act);
            }
            65..72 => {
                let cols = st.project.columns();
                let c = self.rand(cols);
                st.trigger_column(c, act);
            }
            // A presentation on Program (triggered like any slot above): click through it.
            72..90 if st.present.active() => {
                let k = [Key::Next, Key::Next, Key::Next, Key::Prev, Key::Hide][self.rand(5)];
                st.present.keys.push(k);
            }
            90..95 => {
                st.present.pointer = [PointerMode::Off, PointerMode::Dot, PointerMode::Spotlight][self.rand(3)];
                st.present.pointer_pos = (self.rand(1000) as f32 / 1000.0, self.rand(1000) as f32 / 1000.0);
            }
            _ => act.commands.push(Command::SetBpm(90.0 + self.rand(60) as f64)),
        }
        self.actions += 1;
    }

    /// One UI frame. True when the soak is over (summary written).
    pub fn tick(&mut self, st: &mut UiState, snap: &Snapshot, act: &mut Actions) -> bool {
        let now = Instant::now();
        if now >= self.next_action {
            self.act(st, act);
            self.next_action = now + Duration::from_millis(1500 + self.rand(2500) as u64);
        }
        if now >= self.next_log {
            self.next_log += Duration::from_secs(60);
            let (private, ws) = process_memory_mb();
            self.memory.push(private);
            let errors = snap.layers.iter().filter(|l| l.error.is_some()).count();
            let line = format!(
                "{},{private:.1},{ws:.1},{:.1},{:.1},{:.2},{},{},{},{errors}\n",
                self.memory.len(),
                snap.gpu_memory_mb,
                snap.fps,
                snap.p99_ms,
                snap.dropped,
                snap.device_resets,
                self.actions
            );
            self.append(&line);
            st.status = format!("SOAK {} min — {line}", self.memory.len());
        }
        if now < self.until {
            return false;
        }
        let n = 30.min(self.memory.len() / 2);
        let summary = match growth(&self.memory, n) {
            Some((first, last)) => format!(
                "# summary: {:.1} h, {} actions, private memory first {n} min {first:.1} MB, last {n} min {last:.1} MB ({:+.1} MB), peak {:.1} MB\n",
                self.started.elapsed().as_secs_f64() / 3600.0,
                self.actions,
                last - first,
                self.memory.iter().cloned().fold(0.0, f64::max)
            ),
            None => format!("# summary: {} actions, too short for a memory trend\n", self.actions),
        };
        self.append(&summary);
        println!("{summary}");
        true
    }

    fn append(&self, line: &str) {
        if let Some(Ok(mut f)) = self.log.as_ref().map(|l| std::fs::OpenOptions::new().append(true).open(l)) {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_trend() {
        assert_eq!(growth(&[1.0, 2.0, 3.0], 2), None);
        assert_eq!(growth(&[1.0, 1.0, 5.0, 5.0], 2), Some((1.0, 5.0)));
        assert!(process_memory_mb().0 > 0.0);
        let t0 = process_cpu_time();
        // Spin for 100 ms of wall time (CPU time has ~16 ms granularity).
        let start = Instant::now();
        let mut x = 0u64;
        while start.elapsed() < Duration::from_millis(100) {
            x = std::hint::black_box(x.wrapping_add(1));
        }
        assert!(process_cpu_time() > t0, "busy work shows up as CPU time");
    }
}
