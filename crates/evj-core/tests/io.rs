use evj_core::io::{RECENT_MAX, add_recent, load, load_recent, missing_media, relink, relative_to, save};
use evj_core::model::*;
use std::path::{Path, PathBuf};

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("evj-io-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn relative_paths() {
    let base = Path::new("E:/Show/proj");
    assert_eq!(relative_to(Path::new("E:/Show/proj/media/a.mp4"), base), Some(PathBuf::from("media/a.mp4")));
    assert_eq!(relative_to(Path::new("E:/Show/media/a.mp4"), base), Some(PathBuf::from("../media/a.mp4")));
    assert_eq!(relative_to(Path::new("D:/other/a.mp4"), base), None, "different drive stays absolute");
}

#[test]
fn save_load_round_trip_with_relative_media() {
    let dir = tmp("rt");
    std::fs::create_dir_all(dir.join("media")).unwrap();
    let media = dir.join("media").join("a.mov");
    std::fs::write(&media, b"x").unwrap();
    let mut p = Project::new_default();
    p.decks[0].place(0, 0, &[media.clone(), PathBuf::from("Z:/elsewhere/b.mp4")]);
    let file = dir.join("show.vjproj");
    save(&p, &file).unwrap();

    let text = std::fs::read_to_string(&file).unwrap();
    let dir_json = serde_json::to_string(&dir.to_string_lossy()).unwrap();
    assert!(text.contains("media") && !text.contains(dir_json.trim_matches('"')), "stored relative: {text}");
    assert!(text.contains("Z:"), "other drive stays absolute");

    // Move the whole folder: relative media still resolves.
    let moved = tmp("rt-moved");
    std::fs::remove_dir_all(&moved).unwrap();
    std::fs::rename(&dir, &moved).unwrap();
    let back = load(&moved.join("show.vjproj")).unwrap();
    let clip = back.decks[0].clip(0, 0).unwrap();
    assert!(clip.path.exists(), "{:?}", clip.path);
    assert_eq!(back.decks[0].clip(0, 1).unwrap().path, PathBuf::from("Z:/elsewhere/b.mp4"));
}

#[test]
fn save_leaves_no_temp_files() {
    let dir = tmp("atomic");
    let file = dir.join("p.vjproj");
    save(&Project::new_default(), &file).unwrap();
    save(&Project::new_default(), &file).unwrap(); // overwrite works
    let names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, vec![std::ffi::OsString::from("p.vjproj")]);
}

#[test]
fn load_errors_are_reported() {
    let dir = tmp("bad");
    let f = dir.join("bad.vjproj");
    std::fs::write(&f, b"{ not json").unwrap();
    assert!(load(&f).is_err());
    assert!(load(&dir.join("none.vjproj")).is_err());
}

#[test]
fn missing_and_relink() {
    let dir = tmp("relink");
    std::fs::create_dir_all(dir.join("new/sub")).unwrap();
    std::fs::write(dir.join("new/sub/a.mov"), b"x").unwrap();
    let mut p = Project::new_default();
    p.decks[0].place(0, 0, &[PathBuf::from("C:/gone/a.mov"), PathBuf::from("C:/gone/zzz.mov")]);
    assert_eq!(missing_media(&p).len(), 2);
    assert_eq!(relink(&mut p, &dir), 1);
    assert_eq!(p.decks[0].clip(0, 0).unwrap().path, dir.join("new/sub/a.mov"));
    assert_eq!(missing_media(&p), vec![PathBuf::from("C:/gone/zzz.mov")]);
}

#[test]
fn materi_decks_are_saved_relative_and_relinked() {
    use evj_core::model::MateriItem;
    let dir = tmp("materi");
    std::fs::create_dir_all(dir.join("decks/talk")).unwrap();
    let deck = dir.join("decks/talk/deck.json");
    std::fs::write(&deck, "{}").unwrap();
    let mut p = Project::new_default();
    p.materi.push(MateriItem { title: "Sambutan".into(), deck: deck.clone(), done: false });
    save(&p, &dir.join("show.vjproj")).unwrap();
    let text = std::fs::read_to_string(dir.join("show.vjproj")).unwrap();
    assert!(text.contains("decks") && !text.contains(&*dir.file_name().unwrap().to_string_lossy()), "{text}");
    let back = load(&dir.join("show.vjproj")).unwrap();
    assert_eq!(back.materi[0].deck, deck);
    assert_eq!(back.materi[0].title, "Sambutan");
    assert_eq!(back.presentation_layer(), 3, "default: the top layer");
}

#[test]
fn recent_shows_newest_first_without_duplicates() {
    let dir = tmp("recent");
    let f = dir.join("recent.json");
    assert!(load_recent(&f).is_empty(), "no file yet");
    add_recent(&f, Path::new("C:/a.vjproj"));
    add_recent(&f, Path::new("C:/b.vjproj"));
    let list = add_recent(&f, Path::new("c:/A.vjproj"));
    assert_eq!(list, vec![PathBuf::from("c:/A.vjproj"), PathBuf::from("C:/b.vjproj")], "re-opened show moves to the top once");
    for i in 0..20 {
        add_recent(&f, &PathBuf::from(format!("C:/s{i}.vjproj")));
    }
    let list = load_recent(&f);
    assert_eq!(list.len(), RECENT_MAX);
    assert_eq!(list[0], PathBuf::from("C:/s19.vjproj"));
}

#[test]
fn attached_audio_is_media() {
    let mut p = Project::new_default();
    let mut c = Clip::new("C:/m/photo.png".into());
    c.attached = Some(AttachedAudio { path: "C:/m/music.mp3".into(), mix: false, volume: 1.0 });
    p.decks[0].slots[0][0] = Some(c);
    let paths: Vec<_> = evj_core::io::media_paths(&p).cloned().collect();
    assert!(paths.contains(&PathBuf::from("C:/m/music.mp3")));
    assert!(missing_media(&p).contains(&PathBuf::from("C:/m/music.mp3")));
}
