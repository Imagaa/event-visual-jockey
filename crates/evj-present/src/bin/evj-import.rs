//! Isolated importer, run by EVJ: `evj-import <pdf|pptx> <input> <out-dir> <width> <height>`.
//! Exit 0 = `<out-dir>/deck.json` written; otherwise the reason is on stdout.
//! Runs in its own process because Office automation can hang and Windows' PDF renderer crashes
//! while unloading — neither may touch the show.
use std::io::Write;
use std::path::PathBuf;
use windows::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};

fn run(args: &[String]) -> anyhow::Result<()> {
    let [kind, input, out, w, h] = args else { anyhow::bail!("usage: evj-import <pdf|pptx> <input> <out-dir> <width> <height>") };
    let (input, out, w, h) = (PathBuf::from(input), PathBuf::from(out), w.parse()?, h.parse()?);
    match kind.as_str() {
        "pdf" => evj_present::pdf::import_pdf(&input, &out, w, h).map(|_| ()),
        "pptx" => evj_present::pptx::import_pptx(&input, &out, w, h).map(|_| ()),
        other => anyhow::bail!("unknown import kind '{other}'"),
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match run(&args) {
        Ok(()) => {
            println!("ok");
            0
        }
        Err(e) => {
            println!("{e:#}");
            1
        }
    };
    let _ = std::io::stdout().flush();
    // Skip DLL teardown (the PDF renderer crashes there).
    unsafe {
        let _ = TerminateProcess(GetCurrentProcess(), code);
    }
}
