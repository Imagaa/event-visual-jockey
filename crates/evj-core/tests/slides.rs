use evj_core::slides::{MANIFEST, Slide, SlideDeck, SlidePos};
use std::path::PathBuf;

fn deck() -> SlideDeck {
    let s = |n: usize, steps: Vec<f64>| Slide {
        image: PathBuf::from(format!("slide{n:03}.png")),
        video: (steps.len() > 1).then(|| PathBuf::from(format!("slide{n:03}.mp4"))),
        steps,
        end: 5.0,
        notes: format!("notes {n}"),
    };
    SlideDeck { title: "Sambutan".into(), source: PathBuf::from("C:/m/s.pptx"), width: 1920, height: 1080, slides: vec![s(1, vec![0.0]), s(2, vec![0.0, 1.5, 3.0]), s(3, vec![0.0])] }
}

fn p(slide: usize, step: usize) -> SlidePos {
    SlidePos { slide, step }
}

#[test]
fn next_walks_click_steps_then_slides() {
    let d = deck();
    assert_eq!(d.next(p(0, 0)), Some(p(1, 0)));
    assert_eq!(d.next(p(1, 0)), Some(p(1, 1)));
    assert_eq!(d.next(p(1, 1)), Some(p(1, 2)));
    assert_eq!(d.next(p(1, 2)), Some(p(2, 0)));
    assert_eq!(d.next(p(2, 0)), None, "end of the deck");
}

#[test]
fn prev_goes_back_one_step_or_to_the_end_of_the_previous_slide() {
    let d = deck();
    assert_eq!(d.prev(p(1, 2)), Some(p(1, 1)));
    assert_eq!(d.prev(p(1, 0)), Some(p(0, 0)));
    assert_eq!(d.prev(p(2, 0)), Some(p(1, 2)), "previous slide fully built");
    assert_eq!(d.prev(p(0, 0)), None);
}

#[test]
fn segments_of_the_animation_video() {
    let d = deck();
    assert_eq!(d.segment(p(1, 0)), (0.0, 1.5));
    assert_eq!(d.segment(p(1, 1)), (1.5, 3.0));
    assert_eq!(d.segment(p(1, 2)), (3.0, 5.0));
}

#[test]
fn manifest_round_trip_with_relative_media() {
    let dir = std::env::temp_dir().join(format!("evj-deck-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = deck();
    d.save(&dir).unwrap();
    let text = std::fs::read_to_string(dir.join(MANIFEST)).unwrap();
    assert!(text.contains("slide002.mp4") && !text.contains(&*dir.to_string_lossy()), "relative paths");
    let back = SlideDeck::load(&dir.join(MANIFEST)).unwrap();
    assert_eq!(back.slides.len(), 3);
    assert_eq!(back.slides[1].video.as_deref(), Some(dir.join("slide002.mp4").as_path()));
    assert_eq!(back.slides[0].image, dir.join("slide001.png"));
    assert!(SlideDeck::is_deck(&dir.join(MANIFEST)));
    assert!(!SlideDeck::is_deck(&dir.join("x.mp4")));
}

#[test]
fn presentation_files_are_recognised() {
    use evj_core::slides::is_presentation;
    for f in ["a.pptx", "B.PDF", "c.ppt", "d.odp", "e.ppsx"] {
        assert!(is_presentation(std::path::Path::new(f)), "{f}");
    }
    assert!(!is_presentation(std::path::Path::new("deck.json")));
    assert!(!is_presentation(std::path::Path::new("a.mp4")));
}
