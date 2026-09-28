//! PPTX → slides via PowerPoint (COM automation from a PowerShell script, no window shown).
use anyhow::{Context, Result, bail, ensure};
use evj_core::slides::{Slide, SlideDeck};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;

const SCRIPT: &str = include_str!("pptx.ps1");
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn unhex(s: &str) -> String {
    let bytes: Vec<u8> = (0..s.len() / 2).filter_map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).ok()).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Parses the script's `SLIDE|image|video|steps|end|notes-hex` lines.
pub fn parse_slides(stdout: &str, dir: &Path) -> Result<Vec<Slide>> {
    let mut slides = Vec::new();
    for line in stdout.lines().filter_map(|l| l.trim().strip_prefix("SLIDE|")) {
        let f: Vec<&str> = line.split('|').collect();
        ensure!(f.len() == 5, "bad export line: {line}");
        let mut steps: Vec<f64> = f[2].split(',').filter(|s| !s.is_empty()).map(str::parse).collect::<Result<_, _>>()?;
        if steps.is_empty() {
            steps.push(0.0);
        }
        let video = (f[1] != "-").then(|| dir.join(f[1]));
        slides.push(Slide { image: dir.join(f[0]), video: video.clone(), steps: if video.is_some() { steps } else { vec![0.0] }, end: f[3].parse()?, notes: unhex(f[4]) });
    }
    Ok(slides)
}

/// Windows PowerShell by absolute path (PATH may not contain it).
pub fn powershell() -> std::path::PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    std::path::PathBuf::from(root).join("System32").join("WindowsPowerShell").join("v1.0").join("powershell.exe")
}

pub fn powerpoint_installed() -> bool {
    Command::new(powershell())
        .args(["-NoProfile", "-Command", "if ([Type]::GetTypeFromProgID('PowerPoint.Application')) { exit 0 } else { exit 1 }"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .is_ok_and(|s| s.success())
}

/// LibreOffice's `soffice.exe`, if installed.
pub fn libreoffice() -> Option<std::path::PathBuf> {
    ["ProgramFiles", "ProgramFiles(x86)"]
        .iter()
        .filter_map(std::env::var_os)
        .map(|pf| std::path::PathBuf::from(pf).join("LibreOffice").join("program").join("soffice.exe"))
        .find(|p| p.exists())
}

/// PowerPoint when present (full fidelity + animations); otherwise LibreOffice → PDF (static slides).
pub fn import_pptx(pptx: &Path, out_dir: &Path, width: u32, height: u32) -> Result<SlideDeck> {
    let pptx = std::path::absolute(pptx)?;
    ensure!(pptx.exists(), "{} not found", pptx.display());
    let has_powerpoint = std::env::var_os("EVJ_NO_POWERPOINT").is_none() && powerpoint_installed();
    if !has_powerpoint {
        let Some(soffice) = libreoffice() else {
            bail!("PPTX import needs Microsoft PowerPoint or LibreOffice on this laptop. Or save the presentation as PDF and import the PDF.");
        };
        let tmp = std::env::temp_dir().join(format!("evj-lo-{}", std::process::id()));
        std::fs::create_dir_all(&tmp)?;
        let st = Command::new(soffice).args(["--headless", "--convert-to", "pdf", "--outdir"]).arg(&tmp).arg(&pptx).creation_flags(CREATE_NO_WINDOW).status()?;
        let pdf = tmp.join(pptx.file_stem().unwrap_or_default()).with_extension("pdf");
        ensure!(st.success() && pdf.exists(), "LibreOffice could not convert {}", pptx.display());
        let mut deck = crate::pdf::import_pdf(&pdf, out_dir, width, height)?;
        deck.source = pptx.clone();
        deck.save(&std::path::absolute(out_dir)?)?;
        let _ = std::fs::remove_dir_all(&tmp);
        return Ok(deck);
    }
    std::fs::create_dir_all(out_dir)?;
    let out_dir = std::path::absolute(out_dir)?;
    let script = std::env::temp_dir().join(format!("evj-pptx-{}.ps1", std::process::id()));
    // UTF-8 BOM so Windows PowerShell 5.1 reads the script as UTF-8.
    std::fs::write(&script, [&[0xEF, 0xBB, 0xBF][..], SCRIPT.as_bytes()].concat())?;
    let out = Command::new(powershell())
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg(&pptx)
        .arg(&out_dir)
        .args([width.to_string(), height.to_string()])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .context("cannot start PowerShell")?;
    let _ = std::fs::remove_file(&script);
    let stdout = String::from_utf8_lossy(&out.stdout);
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("PowerPoint export failed: {}", err.lines().find(|l| !l.trim().is_empty()).unwrap_or("unknown error"));
    }
    let slides = parse_slides(&stdout, &out_dir)?;
    ensure!(!slides.is_empty(), "the presentation has no slides");
    let title = pptx.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let deck = SlideDeck { title, source: pptx.clone(), width, height, slides };
    deck.save(&out_dir)?;
    Ok(deck)
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_export_lines() {
        let dir = std::path::Path::new("C:/d");
        let out = "noise\nSLIDE|slide001.png|-|0|0.3|48656c6c6f\nSLIDE|slide002.png|slide002.mp4|0,0,0.5|1.3|\n";
        let s = super::parse_slides(out, dir).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].notes, "Hello");
        assert_eq!(s[0].video, None);
        assert_eq!(s[1].video.as_deref(), Some(dir.join("slide002.mp4").as_path()));
        assert_eq!(s[1].steps, vec![0.0, 0.0, 0.5]);
        assert!(super::parse_slides("SLIDE|a|b\n", dir).is_err());
    }
}
