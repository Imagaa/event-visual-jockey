//! `.vjproj` files: media paths relative to the project folder, atomic writes, missing-media relink.
use crate::model::Project;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

pub const EXTENSION: &str = "vjproj";

/// Every media path the project refers to.
pub fn media_paths_mut(p: &mut Project) -> impl Iterator<Item = &mut PathBuf> {
    let clips = p.decks.iter_mut().flat_map(|d| d.slots.iter_mut().flatten().flatten()).flat_map(|c| std::iter::once(&mut c.path).chain(c.attached.as_mut().map(|a| &mut a.path)));
    clips.chain(p.materi.iter_mut().map(|m| &mut m.deck))
}

pub fn media_paths(p: &Project) -> impl Iterator<Item = &PathBuf> {
    let clips = p.decks.iter().flat_map(|d| d.slots.iter().flatten().flatten()).flat_map(|c| std::iter::once(&c.path).chain(c.attached.as_ref().map(|a| &a.path)));
    clips.chain(p.materi.iter().map(|m| &m.deck))
}

fn same(a: &Component, b: &Component) -> bool {
    a.as_os_str().to_ascii_lowercase() == b.as_os_str().to_ascii_lowercase()
}

/// `path` relative to `base` (may climb with `..`); None when they are on different drives.
pub fn relative_to(path: &Path, base: &Path) -> Option<PathBuf> {
    let (p, b): (Vec<_>, Vec<_>) = (path.components().collect(), base.components().collect());
    if p.is_empty() || b.is_empty() || !same(&p[0], &b[0]) {
        return None;
    }
    let common = p.iter().zip(&b).take_while(|(x, y)| same(x, y)).count();
    let mut out: PathBuf = std::iter::repeat_n(Component::ParentDir, b.len() - common).collect();
    out.extend(&p[common..]);
    Some(out)
}

/// Resolves `..` and `.` without touching the file system.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            c => out.push(c),
        }
    }
    out
}

/// Writes via a temp file + rename, so a crash never leaves a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes).with_context(|| format!("write {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

pub fn save(project: &Project, path: &Path) -> Result<()> {
    let base = path.parent().unwrap_or(Path::new("."));
    let mut p = project.clone();
    for m in media_paths_mut(&mut p) {
        if m.is_absolute() {
            if let Some(r) = relative_to(m, base) {
                *m = r;
            }
        }
    }
    write_atomic(path, serde_json::to_string_pretty(&p)?.as_bytes())
}

pub fn load(path: &Path) -> Result<Project> {
    let text = std::fs::read_to_string(path).with_context(|| format!("open {}", path.display()))?;
    let mut p: Project = serde_json::from_str(&text).with_context(|| format!("{} is not a valid project", path.display()))?;
    let base = path.parent().unwrap_or(Path::new("."));
    for m in media_paths_mut(&mut p) {
        if m.is_relative() {
            *m = normalize(&base.join(&*m));
        }
    }
    Ok(p)
}

pub fn missing_media(p: &Project) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = media_paths(p).filter(|m| !m.exists()).cloned().collect();
    v.sort();
    v.dedup();
    v
}

/// Points missing media at files with the same name found under `folder`. Returns how many were fixed.
pub fn relink(p: &mut Project, folder: &Path) -> usize {
    let mut index: HashMap<OsString, PathBuf> = HashMap::new();
    let mut stack = vec![(folder.to_path_buf(), 0)];
    let mut seen = 0usize;
    // ponytail: bounded walk (depth 10, 200k entries) — enough for a media drive, never hangs on C:\
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            seen += 1;
            if seen > 200_000 {
                break;
            }
            let path = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() && depth < 10 => stack.push((path, depth + 1)),
                Ok(t) if t.is_file() => {
                    index.entry(e.file_name().to_ascii_lowercase()).or_insert(path);
                }
                _ => {}
            }
        }
    }
    let mut fixed = 0;
    for m in media_paths_mut(p) {
        if m.exists() {
            continue;
        }
        if let Some(found) = m.file_name().and_then(|n| index.get(&n.to_ascii_lowercase())) {
            *m = found.clone();
            fixed += 1;
        }
    }
    fixed
}

/// `%APPDATA%\EVJ` (created on demand).
pub fn app_dir() -> Option<PathBuf> {
    let d = PathBuf::from(std::env::var_os("APPDATA")?).join("EVJ");
    std::fs::create_dir_all(&d).ok()?;
    Some(d)
}

/// Recently opened / saved shows, newest first.
pub const RECENT_MAX: usize = 10;

pub fn recent_file() -> Option<PathBuf> {
    Some(app_dir()?.join("recent.json"))
}

pub fn load_recent(file: &Path) -> Vec<PathBuf> {
    std::fs::read_to_string(file).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Moves `show` to the top of the list in `file` (no duplicates, at most [`RECENT_MAX`]).
pub fn add_recent(file: &Path, show: &Path) -> Vec<PathBuf> {
    let key = |p: &Path| p.to_string_lossy().to_lowercase();
    let mut list = load_recent(file);
    list.retain(|p| key(p) != key(show));
    list.insert(0, show.to_path_buf());
    list.truncate(RECENT_MAX);
    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = write_atomic(file, json.as_bytes());
    }
    list
}

pub fn autosave_path() -> Option<PathBuf> {
    Some(app_dir()?.join(format!("autosave.{EXTENSION}")))
}
