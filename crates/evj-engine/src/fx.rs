//! Effect & transition library: built-ins compiled into the exe plus user folders, hot-reloaded.
use evj_core::effect::{EffectMeta, Kind, build_shader, parse_meta};
use evj_render::{FxProgram, Gpu};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

macro_rules! builtin {
    ($($file:literal),* $(,)?) => { &[$(($file, include_str!(concat!("../../../effects/", $file)))),*] };
}

const BUILTIN: &[(&str, &str)] = builtin!(
    "transform.hlsl",
    "brightness_contrast.hlsl",
    "hue_shift.hlsl",
    "colorize.hlsl",
    "invert.hlsl",
    "blur.hlsl",
    "kaleidoscope.hlsl",
    "mirror.hlsl",
    "rgb_shift.hlsl",
    "pixelate.hlsl",
    "posterize.hlsl",
    "edge.hlsl",
    "strobe.hlsl",
    "wave.hlsl",
    "feedback.hlsl",
    "vignette.hlsl",
    "chroma_key.hlsl",
    "test_pattern.hlsl",
    "pointer.hlsl",
    "transitions/crossfade.hlsl",
    "transitions/dip_to_black.hlsl",
    "transitions/flash.hlsl",
    "transitions/wipe.hlsl",
    "transitions/slide.hlsl",
    "transitions/zoom.hlsl",
    "transitions/dissolve.hlsl",
    "transitions/luma_fade.hlsl",
    "transitions/blur_fade.hlsl",
    "transitions/pixelate.hlsl",
    "transitions/radial.hlsl",
    "transitions/additive.hlsl",
);

#[derive(Debug, Clone, PartialEq)]
pub enum Source {
    Builtin,
    File { path: PathBuf, modified: Option<SystemTime> },
}

pub struct Entry {
    pub meta: EffectMeta,
    /// None when the shader failed to compile (see `error`).
    pub program: Option<FxProgram>,
    pub error: Option<String>,
    pub source: Source,
}

/// What the UI needs to know about an effect.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectInfo {
    pub meta: EffectMeta,
    pub builtin: bool,
    pub error: Option<String>,
}

pub struct Library {
    pub entries: Vec<Entry>,
    folders: Vec<PathBuf>,
    /// Bumped whenever the list changes.
    pub version: u64,
}

fn load(gpu: &Gpu, src: &str, file: &str, source: Source) -> Entry {
    match parse_meta(src) {
        Ok(meta) => {
            let (program, error) = match FxProgram::compile(gpu, &build_shader(&meta, src, file)) {
                Ok(p) => (Some(p), None),
                Err(e) => (None, Some(format!("{e:#}"))),
            };
            Entry { meta, program, error, source }
        }
        Err(e) => {
            let name = Path::new(file).file_stem().map_or(file.to_string(), |s| s.to_string_lossy().into_owned());
            let meta = EffectMeta { name, params: Vec::new(), feedback: false, kind: Kind::Effect, hidden: false };
            Entry { meta, program: None, error: Some(format!("{e:#}")), source }
        }
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

impl Library {
    pub fn new(gpu: &Gpu, folders: Vec<PathBuf>) -> Library {
        let entries = BUILTIN.iter().map(|(file, src)| load(gpu, src, file, Source::Builtin)).collect();
        let mut lib = Library { entries, folders, version: 0 };
        lib.rescan(gpu);
        lib
    }

    /// Picks up new, changed and deleted `.hlsl` files in the user folders.
    pub fn rescan(&mut self, gpu: &Gpu) {
        let mut files: Vec<PathBuf> = self
            .folders
            .iter()
            .flat_map(|f| std::fs::read_dir(f).into_iter().flatten().flatten())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("hlsl")))
            .collect();
        files.sort();
        let before = self.entries.len();
        let mut changed = false;
        self.entries.retain(|e| match &e.source {
            Source::File { path, .. } => {
                let keep = files.contains(path);
                changed |= !keep;
                keep
            }
            Source::Builtin => true,
        });
        for path in files {
            let m = modified(&path);
            let known = self.entries.iter().position(|e| matches!(&e.source, Source::File { path: p, .. } if *p == path));
            if let Some(i) = known {
                if matches!(&self.entries[i].source, Source::File { modified, .. } if *modified == m) {
                    continue;
                }
                self.entries.remove(i);
            }
            let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let entry = match std::fs::read_to_string(&path) {
                Ok(src) => load(gpu, &src, &file, Source::File { path: path.clone(), modified: m }),
                Err(e) => load(gpu, "", &file, Source::File { path: path.clone(), modified: m }).with_error(format!("cannot read: {e}")),
            };
            // A user file overrides a built-in of the same name and kind.
            self.entries.retain(|e| !(e.source == Source::Builtin && e.meta.name == entry.meta.name && e.meta.kind == entry.meta.kind));
            self.entries.push(entry);
            changed = true;
        }
        if changed || self.entries.len() != before {
            self.version += 1;
        }
    }

    /// An effect (not a transition) by name.
    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.find_kind(name, Kind::Effect)
    }

    pub fn find_transition(&self, name: &str) -> Option<&Entry> {
        self.find_kind(name, Kind::Transition)
    }

    fn find_kind(&self, name: &str, kind: Kind) -> Option<&Entry> {
        self.entries.iter().rev().find(|e| e.meta.name == name && e.meta.kind == kind)
    }

    pub fn transitions(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.meta.kind == Kind::Transition)
    }

    pub fn infos(&self) -> Vec<EffectInfo> {
        let mut v: Vec<EffectInfo> = self
            .entries
            .iter()
            .map(|e| EffectInfo { meta: e.meta.clone(), builtin: e.source == Source::Builtin, error: e.error.clone() })
            .collect();
        v.sort_by(|a, b| a.meta.name.cmp(&b.meta.name));
        v
    }
}

impl Entry {
    fn with_error(mut self, e: String) -> Entry {
        self.error = Some(e);
        self.program = None;
        self
    }
}
