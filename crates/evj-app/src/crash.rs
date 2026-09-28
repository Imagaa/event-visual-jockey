//! Crash reports in `%APPDATA%\EVJ\crash`: Rust panics (message + backtrace) and native crashes (minidump).
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::Win32::Foundation::{GENERIC_WRITE, HANDLE};
use windows::Win32::Storage::FileSystem::{CREATE_ALWAYS, CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_NONE};
use windows::Win32::System::Diagnostics::Debug::{
    EXCEPTION_POINTERS, MINIDUMP_EXCEPTION_INFORMATION, MiniDumpWithIndirectlyReferencedMemory, MiniDumpWriteDump, SetUnhandledExceptionFilter,
};
use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId};
use windows::core::HSTRING;

static DIR: OnceLock<PathBuf> = OnceLock::new();

fn stamp() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Installs the panic hook and the native crash handler. Call once at start-up.
pub fn install(dir: &Path) {
    let _ = std::fs::create_dir_all(dir);
    let _ = DIR.set(dir.to_path_buf());
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current().name().unwrap_or("?").to_string();
        let text = format!("EVJ {} panic in thread '{thread}'\n{info}\n\n{}", env!("CARGO_PKG_VERSION"), std::backtrace::Backtrace::force_capture());
        if let Some(d) = DIR.get() {
            let _ = std::fs::write(d.join(format!("panic-{}.txt", stamp())), &text);
        }
        evj_core::log::error("panic", &format!("thread '{thread}': {info}"));
        previous(info);
    }));
    unsafe {
        SetUnhandledExceptionFilter(Some(write_minidump));
    }
}

unsafe extern "system" fn write_minidump(info: *const EXCEPTION_POINTERS) -> i32 {
    const EXCEPTION_CONTINUE_SEARCH: i32 = 0;
    let Some(dir) = DIR.get() else { return EXCEPTION_CONTINUE_SEARCH };
    let path = HSTRING::from(dir.join(format!("crash-{}.dmp", stamp())).as_os_str());
    unsafe {
        let Ok(file) = CreateFileW(&path, GENERIC_WRITE.0, FILE_SHARE_NONE, None, CREATE_ALWAYS, FILE_ATTRIBUTE_NORMAL, Some(HANDLE::default())) else {
            return EXCEPTION_CONTINUE_SEARCH;
        };
        let exception = MINIDUMP_EXCEPTION_INFORMATION { ThreadId: GetCurrentThreadId(), ExceptionPointers: info as *mut _, ClientPointers: false.into() };
        let _ = MiniDumpWriteDump(GetCurrentProcess(), GetCurrentProcessId(), file, MiniDumpWithIndirectlyReferencedMemory, Some(&exception), None, None);
        let _ = windows::Win32::Foundation::CloseHandle(file);
    }
    EXCEPTION_CONTINUE_SEARCH
}

#[cfg(test)]
mod tests {
    #[test]
    fn panics_leave_a_report() {
        let dir = std::env::temp_dir().join(format!("evj-crash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        super::install(&dir);
        let _ = std::thread::Builder::new().name("boom".into()).spawn(|| panic!("test panic 42")).unwrap().join();
        let report = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .find(|e| e.file_name().to_string_lossy().starts_with("panic-"))
            .map(|e| std::fs::read_to_string(e.path()).unwrap())
            .expect("panic report written");
        assert!(report.contains("test panic 42") && report.contains("'boom'"), "{report}");
    }
}
