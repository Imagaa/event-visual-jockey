//! PDF → slide images with Windows' own PDF renderer (Windows.Data.Pdf, Windows 10+).
use anyhow::{Context, Result, ensure};
use evj_core::slides::{Slide, SlideDeck};
use std::path::Path;
use windows::Data::Pdf::{PdfDocument, PdfPageRenderOptions};
use windows::Storage::{CreationCollisionOption, FileAccessMode, StorageFile, StorageFolder};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::HSTRING;

/// Renders every page of `pdf` into `out_dir` (fitted into `width`×`height`) and writes `deck.json`.
pub fn import_pdf(pdf: &Path, out_dir: &Path, width: u32, height: u32) -> Result<SlideDeck> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
    let pdf = std::path::absolute(pdf)?;
    std::fs::create_dir_all(out_dir)?;
    let out_dir = std::path::absolute(out_dir)?;
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(pdf.as_os_str()))?.join().with_context(|| format!("open {}", pdf.display()))?;
    let doc = PdfDocument::LoadFromFileAsync(&file)?.join().context("not a readable PDF")?;
    let pages = doc.PageCount()?;
    ensure!(pages > 0, "the PDF has no pages");
    let folder = StorageFolder::GetFolderFromPathAsync(&HSTRING::from(out_dir.as_os_str()))?.join()?;
    let mut slides = Vec::new();
    for i in 0..pages {
        let page = doc.GetPage(i)?;
        let size = page.Size()?;
        // Fit the page into the target size, keeping its aspect.
        let k = (width as f32 / size.Width.max(1.0)).min(height as f32 / size.Height.max(1.0));
        let opts = PdfPageRenderOptions::new()?;
        opts.SetDestinationWidth((size.Width * k).round().max(1.0) as u32)?;
        opts.SetDestinationHeight((size.Height * k).round().max(1.0) as u32)?;
        let name = format!("slide{:03}.png", i + 1);
        let out = folder.CreateFileAsync(&HSTRING::from(name.as_str()), CreationCollisionOption::ReplaceExisting)?.join()?;
        let stream = out.OpenAsync(FileAccessMode::ReadWrite)?.join()?;
        page.RenderWithOptionsToStreamAsync(&stream, &opts)?.join().with_context(|| format!("render page {}", i + 1))?;
        stream.FlushAsync()?.join()?;
        stream.Close()?;
        slides.push(Slide { image: out_dir.join(&name), video: None, steps: vec![0.0], end: 0.0, notes: String::new() });
    }
    let title = pdf.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let deck = SlideDeck { title, source: pdf.clone(), width, height, slides };
    deck.save(&out_dir)?;
    Ok(deck)
}
