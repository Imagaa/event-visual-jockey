use evj_core::slides::{MANIFEST, SlideDeck};
use evj_media::image::load_image;
use std::path::PathBuf;

/// A valid PDF with one full-page colour rectangle per page (RGB 0..1).
fn write_pdf(path: &PathBuf, pages: &[[f32; 3]]) {
    let mut objs: Vec<String> = Vec::new();
    let n = pages.len();
    objs.push("<< /Type /Catalog /Pages 2 0 R >>".into());
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 3 + i * 2)).collect();
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" ")));
    for (i, c) in pages.iter().enumerate() {
        objs.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 320 180] /Contents {} 0 R >>", 4 + i * 2));
        let content = format!("{} {} {} rg 0 0 320 180 re f", c[0], c[1], c[2]);
        objs.push(format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()));
    }
    let mut out = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out += &format!("{} 0 obj\n{o}\nendobj\n", i + 1);
    }
    let xref = out.len();
    out += &format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1);
    for off in offsets {
        out += &format!("{off:010} 00000 n \n");
    }
    out += &format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1);
    std::fs::write(path, out).unwrap();
}

/// Runs the isolated importer exactly as EVJ does.
fn import(args: &[&str]) -> Result<SlideDeck, String> {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_evj-import")).args(args).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        Ok(SlideDeck::load(&PathBuf::from(args[2]).join(MANIFEST)).unwrap())
    } else {
        Err(text + &String::from_utf8_lossy(&out.stderr))
    }
}

#[test]
fn pdf_pages_become_slides() {
    let dir = std::env::temp_dir().join(format!("evj-pdf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pdf = dir.join("talk.pdf");
    write_pdf(&pdf, &[[1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]);
    let out = dir.join("deck");
    let deck = import(&["pdf", pdf.to_str().unwrap(), out.to_str().unwrap(), "640", "360"]).unwrap();
    assert_eq!(deck.slides.len(), 2);
    assert_eq!(deck.title, "talk");
    let (w, h, px) = load_image(&deck.slides[0].image, 4096).unwrap();
    assert_eq!((w, h), (640, 360));
    assert!(px[2] > 200 && px[0] < 50, "page 1 red (BGRA): {:?}", &px[..4]);
    let (_, _, px) = load_image(&deck.slides[1].image, 4096).unwrap();
    assert!(px[0] > 200 && px[2] < 50, "page 2 blue: {:?}", &px[..4]);
    let back = SlideDeck::load(&out.join(MANIFEST)).unwrap();
    assert_eq!(back, deck);
}

#[test]
fn bad_pdf_is_an_error() {
    let dir = std::env::temp_dir().join(format!("evj-pdf-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dir.join("bad.pdf");
    std::fs::write(&bad, b"not a pdf").unwrap();
    let err = import(&["pdf", bad.to_str().unwrap(), dir.join("deck").to_str().unwrap(), "640", "360"]).unwrap_err();
    assert!(err.contains("PDF"), "{err}");
}
