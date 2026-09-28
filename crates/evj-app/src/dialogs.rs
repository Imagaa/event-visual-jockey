//! Native Windows file dialogs (IFileOpenDialog / IFileSaveDialog). Cancel returns nothing.
//! `spawn` runs one on its own thread, owned by the EVJ window, so it cannot hide behind EVJ
//! and EVJ keeps drawing (monitors, countdowns) while it is open.
use std::path::PathBuf;
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::*;
use windows::core::{HSTRING, Interface, PCWSTR};

pub const MEDIA_FILTER: (&str, &str) =
    ("Media", "*.mov;*.mp4;*.m4v;*.mkv;*.avi;*.wmv;*.webm;*.png;*.jpg;*.jpeg;*.bmp;*.gif;*.tif;*.tiff;*.webp;*.mp3;*.wav;*.m4a;*.aac;*.flac;*.wma");
pub const PROJECT_FILTER: (&str, &str) = ("EVJ project", "*.vjproj");

fn item_path(item: &IShellItem) -> Option<PathBuf> {
    unsafe {
        let p = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as _));
        s.map(PathBuf::from)
    }
}

fn setup(owner: isize, d: &IFileDialog, title: &str, filters: &[(&str, &str)], extra: FILEOPENDIALOGOPTIONS) -> Option<()> {
    unsafe {
        let strings: Vec<(HSTRING, HSTRING)> = filters.iter().map(|(n, s)| (HSTRING::from(*n), HSTRING::from(*s))).collect();
        let specs: Vec<COMDLG_FILTERSPEC> =
            strings.iter().map(|(n, s)| COMDLG_FILTERSPEC { pszName: PCWSTR(n.as_ptr()), pszSpec: PCWSTR(s.as_ptr()) }).collect();
        if !specs.is_empty() {
            d.SetFileTypes(&specs).ok()?;
        }
        d.SetTitle(&HSTRING::from(title)).ok()?;
        let opts = d.GetOptions().ok()?;
        d.SetOptions(opts | FOS_FORCEFILESYSTEM | extra).ok()?;
        let owner = (owner != 0).then_some(HWND(owner as _));
        d.Show(owner).ok() // Err = cancelled
    }
}

pub fn open_files(owner: isize, title: &str, filters: &[(&str, &str)], multi: bool) -> Vec<PathBuf> {
    unsafe {
        let Ok(d) = CoCreateInstance::<_, IFileOpenDialog>(&FileOpenDialog, None, CLSCTX_INPROC_SERVER) else { return vec![] };
        let extra = if multi { FOS_ALLOWMULTISELECT } else { FILEOPENDIALOGOPTIONS(0) };
        let Ok(base) = d.cast::<IFileDialog>() else { return vec![] };
        if setup(owner, &base, title, filters, extra).is_none() {
            return vec![];
        }
        let Ok(items) = d.GetResults() else { return vec![] };
        let n = items.GetCount().unwrap_or(0);
        (0..n).filter_map(|i| items.GetItemAt(i).ok()).filter_map(|it| item_path(&it)).collect()
    }
}

pub fn pick_folder(owner: isize, title: &str) -> Option<PathBuf> {
    unsafe {
        let d = CoCreateInstance::<_, IFileOpenDialog>(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        setup(owner, &d.cast().ok()?, title, &[], FOS_PICKFOLDERS)?;
        item_path(&d.GetResult().ok()?)
    }
}

pub fn save_file(owner: isize, title: &str, filter: (&str, &str), default_name: &str, extension: &str) -> Option<PathBuf> {
    unsafe {
        let d = CoCreateInstance::<_, IFileSaveDialog>(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).ok()?;
        d.SetFileName(&HSTRING::from(default_name)).ok()?;
        d.SetDefaultExtension(&HSTRING::from(extension)).ok()?;
        setup(owner, &d.cast().ok()?, title, &[filter], FOS_OVERWRITEPROMPT)?;
        item_path(&d.GetResult().ok()?)
    }
}

/// A dialog to show on its own thread.
pub enum Ask {
    Open { title: String, filters: Vec<(String, String)>, multi: bool },
    Folder { title: String },
    Save { title: String, filter: (String, String), name: String, ext: String },
}

/// Runs the dialog on its own STA thread, owned by `owner`. Empty result = cancelled.
pub fn spawn(owner: isize, ask: Ask) -> std::sync::mpsc::Receiver<Vec<PathBuf>> {
    let (tx, rx) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new().name("evj-dialog".into()).spawn(move || {
        unsafe {
            let _ = windows::Win32::System::Com::CoInitializeEx(None, windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
        }
        let out = match ask {
            Ask::Open { title, filters, multi } => {
                let f: Vec<(&str, &str)> = filters.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
                open_files(owner, &title, &f, multi)
            }
            Ask::Folder { title } => pick_folder(owner, &title).into_iter().collect(),
            Ask::Save { title, filter, name, ext } => save_file(owner, &title, (&filter.0, &filter.1), &name, &ext).into_iter().collect(),
        };
        let _ = tx.send(out);
    });
    if spawned.is_err() {
        evj_core::log::warn("dialogs", "could not start the dialog thread");
    }
    rx
}

pub fn filters(list: &[(&str, &str)]) -> Vec<(String, String)> {
    list.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}
