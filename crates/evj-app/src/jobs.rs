//! Background "Convert to HAP" jobs.
use evj_media::convert::{HapVariant, convert_to_hap, find_ffmpeg, hap_path};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

struct Job {
    input: PathBuf,
    output: PathBuf,
    /// (progress 0..1, finished with Ok/Err)
    state: Arc<Mutex<(f64, Option<Result<(), String>>)>>,
    _cancel: Arc<AtomicBool>,
}

pub struct Jobs {
    list: Vec<Job>,
}

pub enum Finished {
    Ok { input: PathBuf, output: PathBuf },
    Failed { input: PathBuf, error: String },
}

impl Jobs {
    pub fn new() -> Jobs {
        Jobs { list: Vec::new() }
    }

    pub fn start(&mut self, input: PathBuf, variant: HapVariant, duration: f64) -> Result<(), String> {
        if self.list.iter().any(|j| j.input == input) {
            return Err("already converting".into());
        }
        let ffmpeg = find_ffmpeg().ok_or("ffmpeg.exe not found (install EVJ again or add ffmpeg to PATH)")?;
        let output = hap_path(&input);
        let state = Arc::new(Mutex::new((0.0, None)));
        let cancel = Arc::new(AtomicBool::new(false));
        let (s, c, i, o) = (state.clone(), cancel.clone(), input.clone(), output.clone());
        std::thread::Builder::new()
            .name("evj-convert".into())
            .spawn(move || {
                let r = convert_to_hap(&ffmpeg, &i, &o, variant, duration, &c, |p| {
                    if let Ok(mut st) = s.lock() {
                        st.0 = p;
                    }
                });
                if let Ok(mut st) = s.lock() {
                    st.1 = Some(r.map_err(|e| format!("{e:#}")));
                }
            })
            .map_err(|e| e.to_string())?;
        self.list.push(Job { input, output, state, _cancel: cancel });
        Ok(())
    }

    /// One line per running job, for the status bar.
    pub fn status(&self) -> Vec<String> {
        self.list
            .iter()
            .map(|j| {
                let p = j.state.lock().map(|s| s.0).unwrap_or(0.0);
                let name = j.input.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                format!("HAP: {name} {:.0}%", p * 100.0)
            })
            .collect()
    }

    pub fn poll(&mut self) -> Vec<Finished> {
        let mut done = Vec::new();
        self.list.retain(|j| {
            let result = j.state.lock().ok().and_then(|s| s.1.clone());
            match result {
                Some(Ok(())) => {
                    done.push(Finished::Ok { input: j.input.clone(), output: j.output.clone() });
                    false
                }
                Some(Err(error)) => {
                    done.push(Finished::Failed { input: j.input.clone(), error });
                    false
                }
                None => true,
            }
        });
        done
    }
}
