//! "Convert to HAP" via ffmpeg.exe (bundled next to evj.exe, or on PATH).
use anyhow::{Context, Result, bail};
use std::io::{BufRead, BufReader, Read};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HapVariant {
    /// Smallest, fastest (DXT1).
    Hap,
    /// With transparency (DXT5).
    HapAlpha,
    /// Higher quality (scaled YCoCg DXT5), ~2x the size of Hap.
    HapQ,
}

impl HapVariant {
    fn format(self) -> &'static str {
        match self {
            HapVariant::Hap => "hap",
            HapVariant::HapAlpha => "hap_alpha",
            HapVariant::HapQ => "hap_q",
        }
    }
}

/// ffmpeg.exe next to the running exe, else the one on PATH.
pub fn find_ffmpeg() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok()?.parent()?.join("ffmpeg.exe");
    if beside.exists() {
        return Some(beside);
    }
    let ok = Command::new("ffmpeg").arg("-version").creation_flags(CREATE_NO_WINDOW).stdout(Stdio::null()).stderr(Stdio::null()).status();
    ok.ok().filter(|s| s.success()).map(|_| PathBuf::from("ffmpeg"))
}

/// Default output name: `clip.mp4` → `clip.hap.mov` next to it.
pub fn hap_path(input: &Path) -> PathBuf {
    let stem = input.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "clip".into());
    input.with_file_name(format!("{stem}.hap.mov"))
}

/// Converts `input` to HAP in a .mov (audio kept as PCM). `duration` (seconds) scales `progress` (0..1).
/// Dimensions are padded up to multiples of 4, as HAP requires.
pub fn convert_to_hap(
    ffmpeg: &Path,
    input: &Path,
    output: &Path,
    variant: HapVariant,
    duration: f64,
    cancel: &AtomicBool,
    mut progress: impl FnMut(f64),
) -> Result<()> {
    if cancel.load(Ordering::Relaxed) {
        bail!("cancelled");
    }
    let mut child = Command::new(ffmpeg)
        .args(["-hide_banner", "-nostdin", "-y", "-i"])
        .arg(input)
        .args(["-map", "0:v:0", "-map", "0:a?", "-vf", "pad=ceil(iw/4)*4:ceil(ih/4)*4", "-c:v", "hap", "-format", variant.format()])
        .args(["-chunks", "4", "-c:a", "pcm_s16le", "-progress", "pipe:1", "-nostats"])
        .arg(output)
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("cannot start {}", ffmpeg.display()))?;
    let mut stderr = child.stderr.take().context("ffmpeg stderr")?;
    let errors = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stderr.read_to_string(&mut s);
        s
    });
    let stdout = child.stdout.take().context("ffmpeg stdout")?;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(output);
            bail!("cancelled");
        }
        if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<f64>().ok()) {
            if duration > 0.0 {
                progress((us / 1e6 / duration).clamp(0.0, 1.0));
            }
        } else if line.trim() == "progress=end" {
            progress(1.0);
        }
    }
    let status = child.wait()?;
    let log = errors.join().unwrap_or_default();
    if !status.success() {
        let _ = std::fs::remove_file(output);
        let tail: Vec<&str> = log.lines().filter(|l| !l.trim().is_empty()).rev().take(3).collect();
        bail!("ffmpeg failed: {}", tail.into_iter().rev().collect::<Vec<_>>().join(" | "));
    }
    Ok(())
}
