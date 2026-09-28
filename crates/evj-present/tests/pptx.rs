use evj_core::slides::{MANIFEST, SlideDeck};
use std::path::PathBuf;

/// Builds a 2-slide PPTX with PowerPoint itself: slide 1 has notes, slide 2 two fade-in clicks.
fn make_pptx(path: &PathBuf) -> bool {
    let script = format!(
        r#"
$ErrorActionPreference = 'Stop'
$app = New-Object -ComObject PowerPoint.Application
$p = $app.Presentations.Add(0)
$s1 = $p.Slides.Add(1, 1)
$s1.Shapes.Item(1).TextFrame.TextRange.Text = 'Hello EVJ'
$s1.NotesPage.Shapes.Placeholders.Item(2).TextFrame.TextRange.Text = 'Speaker notes here'
$s2 = $p.Slides.Add(2, 12)
$b1 = $s2.Shapes.AddTextbox(1, 100, 100, 400, 50); $b1.TextFrame.TextRange.Text = 'First point'
$b2 = $s2.Shapes.AddTextbox(1, 100, 200, 400, 50); $b2.TextFrame.TextRange.Text = 'Second point'
[void]$s2.TimeLine.MainSequence.AddEffect($b1, 10, 0, 1)
[void]$s2.TimeLine.MainSequence.AddEffect($b2, 10, 0, 1)
$p.SaveAs('{}')
$p.Close()
if ($app.Presentations.Count -eq 0) {{ $app.Quit() }}
"#,
        path.display()
    );
    std::process::Command::new(evj_present::pptx::powershell()).args(["-NoProfile", "-NonInteractive", "-Command", &script]).status().is_ok_and(|s| s.success())
}

#[test]
fn pptx_slides_notes_and_click_animations() {
    if !evj_present::pptx::powerpoint_installed() {
        eprintln!("PowerPoint not installed: skipped");
        return;
    }
    let dir = std::env::temp_dir().join(format!("evj-pptx-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pptx = dir.join("talk.pptx");
    assert!(make_pptx(&pptx), "could not build the test presentation");
    let out = dir.join("deck");
    let st = std::process::Command::new(env!("CARGO_BIN_EXE_evj-import"))
        .args(["pptx", pptx.to_str().unwrap(), out.to_str().unwrap(), "960", "540"])
        .output()
        .unwrap();
    assert!(st.status.success(), "{}", String::from_utf8_lossy(&st.stdout));
    let d = SlideDeck::load(&out.join(MANIFEST)).unwrap();
    assert_eq!(d.slides.len(), 2);
    assert!(d.slides.iter().all(|s| s.image.exists()));
    assert_eq!(d.slides[0].notes.trim(), "Speaker notes here");
    assert!(d.slides[0].video.is_none(), "no animation, no video");
    let s2 = &d.slides[1];
    assert!(s2.video.as_ref().is_some_and(|v| v.exists()), "animated slide exported as video");
    assert_eq!(s2.steps.len(), 3, "appear + 2 clicks: {:?}", s2.steps);
    assert!(s2.steps[2] > s2.steps[1] && s2.end > s2.steps[2], "{:?} end {}", s2.steps, s2.end);
}

#[test]
fn without_powerpoint_the_fallback_or_a_clear_message() {
    let dir = std::env::temp_dir().join(format!("evj-pptx-fb-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pptx = dir.join("talk.pptx");
    if evj_present::pptx::powerpoint_installed() {
        assert!(make_pptx(&pptx));
    } else {
        return;
    }
    let st = std::process::Command::new(env!("CARGO_BIN_EXE_evj-import"))
        .env("EVJ_NO_POWERPOINT", "1")
        .args(["pptx", pptx.to_str().unwrap(), dir.join("deck").to_str().unwrap(), "960", "540"])
        .output()
        .unwrap();
    let msg = String::from_utf8_lossy(&st.stdout);
    if evj_present::pptx::libreoffice().is_some() {
        assert!(st.status.success(), "{msg}");
    } else {
        assert!(!st.status.success() && msg.contains("LibreOffice") && msg.contains("PDF"), "{msg}");
    }
}
