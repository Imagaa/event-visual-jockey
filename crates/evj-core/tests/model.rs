use evj_core::model::*;
use std::path::PathBuf;

#[test]
fn default_project_shape() {
    let p = Project::new_default();
    assert_eq!((p.composition.width, p.composition.height), (1920, 1080));
    assert_eq!(p.composition.bpm, 120.0);
    assert_eq!(p.composition.layers.len(), 4);
    assert_eq!(p.decks.len(), 1);
    assert!(p.decks[0].slots.iter().all(|row| row.len() == 8));
    assert_eq!(p.decks[0].slots.len(), 4);
}

#[test]
fn json_round_trip() {
    let mut p = Project::new_default();
    p.decks[0].slots[1][2] = Some(Clip::new(PathBuf::from("media/a.mov")));
    p.composition.layers[0].blend = BlendMode::Screen;
    let json = serde_json::to_string(&p).unwrap();
    let back: Project = serde_json::from_str(&json).unwrap();
    assert_eq!(back, p);
}

#[test]
fn unknown_fields_are_ignored() {
    let mut v = serde_json::to_value(Project::new_default()).unwrap();
    v["future_feature"] = serde_json::json!({"x": 1});
    let p: Project = serde_json::from_value(v).unwrap();
    assert_eq!(p.composition.layers.len(), 4);
}

#[test]
fn clip_defaults() {
    let c = Clip::new(PathBuf::from("C:/x/Video Bumper.mp4"));
    assert_eq!(c.name, "Video Bumper");
    assert_eq!((c.mode, c.speed, c.in_point, c.out_point, c.bpm_beats, c.fit), (PlayMode::Loop, 1.0, 0.0, 1.0, None, FitMode::Fit));
}

#[test]
fn blend_mode_indices_are_stable() {
    assert_eq!(BlendMode::ALL.len(), 10);
    for (i, m) in BlendMode::ALL.iter().enumerate() {
        assert_eq!(m.index(), i as u32);
    }
}

#[test]
fn panic_media_defaults_to_none_and_round_trips() {
    let mut p = Project::new_default();
    assert_eq!(p.panic_media, None);
    p.panic_media = Some((0, 1, 2));
    let back: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(back.panic_media, Some((0, 1, 2)));
    let old: Project = serde_json::from_str("{}").unwrap();
    assert_eq!(old.panic_media, None, "old shows load");
}

#[test]
fn an_a_era_show_loads_with_the_new_fields_defaulted() {
    let json = r#"{"decks":[{"name":"D","slots":[[{"path":"C:/a.mov","name":"a"}]]}]}"#;
    let p: Project = serde_json::from_str(json).unwrap();
    let c = p.decks[0].slots[0][0].as_ref().unwrap();
    assert!(c.attached.is_none() && c.still_secs.is_none() && !c.aired);
    assert!(p.decks[0].sequences.is_empty() && p.decks[0].scene_names.is_empty());
}

#[test]
fn materi_moves_into_the_presentation_layer() {
    let mut p = Project::new_default();
    let top = p.composition.layers.len() - 1;
    p.decks[0].slots[top][0] = Some(Clip::new("C:/m/busy.mov".into()));
    p.materi = vec![
        MateriItem { title: "Talk A".into(), deck: "C:/d/a/deck.json".into(), done: true },
        MateriItem { title: "Talk B".into(), deck: "C:/d/b/deck.json".into(), done: false },
    ];
    assert!(p.migrate_materi());
    assert!(p.materi.is_empty());
    let a = p.decks[0].clip(top, 1).unwrap();
    assert_eq!((a.name.as_str(), a.aired), ("Talk A", true), "first empty column of the top layer");
    assert_eq!(p.decks[0].clip(top, 2).unwrap().name, "Talk B");
    assert!(!p.migrate_materi(), "nothing left to migrate");
}
