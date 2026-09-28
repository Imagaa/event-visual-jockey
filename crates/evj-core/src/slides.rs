//! Imported presentations (PPTX / PDF): a folder with one image per slide, optional per-slide
//! animation videos with click boundaries, speaker notes, and a `deck.json` manifest.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "deck.json";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slide {
    /// The fully built slide (all animations done).
    pub image: PathBuf,
    /// Animations / embedded video of this slide, clicks turned into pauses.
    pub video: Option<PathBuf>,
    /// Start of each click step in the video (seconds); step 0 = the slide appears.
    pub steps: Vec<f64>,
    /// Video length (seconds).
    pub end: f64,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlideDeck {
    pub title: String,
    pub source: PathBuf,
    pub width: u32,
    pub height: u32,
    pub slides: Vec<Slide>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SlidePos {
    pub slide: usize,
    pub step: usize,
}

impl Slide {
    fn steps(&self) -> usize {
        self.steps.len().max(1)
    }
}

impl SlideDeck {
    pub fn is_deck(path: &Path) -> bool {
        path.file_name().is_some_and(|n| n.eq_ignore_ascii_case(MANIFEST))
    }

    /// Next click: the next animation step, or the next slide.
    pub fn next(&self, p: SlidePos) -> Option<SlidePos> {
        let s = self.slides.get(p.slide)?;
        if p.step + 1 < s.steps() {
            return Some(SlidePos { slide: p.slide, step: p.step + 1 });
        }
        (p.slide + 1 < self.slides.len()).then_some(SlidePos { slide: p.slide + 1, step: 0 })
    }

    /// Back one step; from the first step, the previous slide fully built.
    pub fn prev(&self, p: SlidePos) -> Option<SlidePos> {
        if p.step > 0 {
            return Some(SlidePos { slide: p.slide, step: p.step - 1 });
        }
        let slide = p.slide.checked_sub(1)?;
        Some(SlidePos { slide, step: self.slides.get(slide)?.steps() - 1 })
    }

    /// Video time range played for a step.
    pub fn segment(&self, p: SlidePos) -> (f64, f64) {
        let Some(s) = self.slides.get(p.slide) else { return (0.0, 0.0) };
        let from = s.steps.get(p.step).copied().unwrap_or(0.0);
        let to = s.steps.get(p.step + 1).copied().unwrap_or(s.end);
        (from, to.max(from))
    }

    /// Writes `dir/deck.json` with media paths relative to `dir`.
    pub fn save(&self, dir: &Path) -> Result<()> {
        let mut d = self.clone();
        let rel = |p: &mut PathBuf| {
            if let Ok(r) = p.strip_prefix(dir) {
                *p = r.to_path_buf();
            }
        };
        for s in &mut d.slides {
            rel(&mut s.image);
            if let Some(v) = s.video.as_mut() {
                rel(v);
            }
        }
        crate::io::write_atomic(&dir.join(MANIFEST), serde_json::to_string_pretty(&d)?.as_bytes())
    }

    pub fn load(manifest: &Path) -> Result<SlideDeck> {
        let dir = manifest.parent().unwrap_or(Path::new("."));
        let text = std::fs::read_to_string(manifest).with_context(|| format!("open {}", manifest.display()))?;
        let mut d: SlideDeck = serde_json::from_str(&text).context("invalid deck.json")?;
        for s in &mut d.slides {
            s.image = dir.join(&s.image);
            if let Some(v) = s.video.as_mut() {
                *v = dir.join(&*v);
            }
        }
        Ok(d)
    }
}

/// PowerPoint / PDF files EVJ imports into a slide deck.
pub fn is_presentation(path: &Path) -> bool {
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase());
    matches!(ext.as_deref(), Some("pptx" | "ppt" | "pptm" | "ppsx" | "pps" | "odp" | "pdf"))
}
