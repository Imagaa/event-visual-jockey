//! Tiny file logger: `%APPDATA%\EVJ\evj.log` (previous session kept as `evj.old.log`).
//! Before `init` (and in tests) lines go to stderr only.
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static FILE: Mutex<Option<File>> = Mutex::new(None);

pub fn init(dir: &Path) {
    let (log, old) = (dir.join("evj.log"), dir.join("evj.old.log"));
    let _ = std::fs::rename(&log, old);
    if let Ok(f) = File::create(&log) {
        if let Ok(mut g) = FILE.lock() {
            *g = Some(f);
        }
    }
}

fn write(level: &str, target: &str, msg: &str) {
    let t = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let line = format!("{t:.3} {level} {target}: {msg}\n");
    eprint!("{line}");
    if let Ok(mut g) = FILE.lock() {
        if let Some(f) = g.as_mut() {
            let _ = f.write_all(line.as_bytes());
            let _ = f.flush();
        }
    }
}

pub fn info(target: &str, msg: &str) {
    write("INFO", target, msg);
}

pub fn warn(target: &str, msg: &str) {
    write("WARN", target, msg);
}

pub fn error(target: &str, msg: &str) {
    write("ERROR", target, msg);
}
