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
    let top = p.presentation_layer();
    assert_eq!(top, 0, "the top layer is Layer 1");
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

#[test]
fn images_have_a_default_duration() {
    let mut c = Clip::new(PathBuf::from("a.png"));
    assert_eq!(c.image_secs(), DEFAULT_IMAGE_SECS);
    c.still_secs = Some(12.5);
    assert_eq!(c.image_secs(), 12.5);
}

#[test]
fn old_shows_are_reversed_so_they_look_the_same() {
    use evj_core::keymap::Action;
    let mut p = Project::new_default();
    assert_eq!(p.layer_order, 1, "new shows use the new order");
    assert!(!p.migrate());
    p.layer_order = 0;
    p.decks[0].slots[3][0] = Some(Clip::new(PathBuf::from("C:/m/top.mov")));
    p.decks[0].slots[0][0] = Some(Clip::new(PathBuf::from("C:/m/bottom.mov")));
    p.composition.layers[3].name = "Titles".into();
    p.keymap.bind("A", Action::TriggerSlot { layer: 3, col: 0 });
    p.keymap.bind("C", Action::ClearLayer(0));
    p.keymap.bind("S", Action::TriggerColumn(2));
    p.panic_media = Some((0, 3, 0));
    p.presentation_layer = Some(1);
    p.outputs[0].source = evj_core::output::OutputSource::Layer(3);
    assert!(p.migrate());
    assert_eq!(p.outputs[0].source, evj_core::output::OutputSource::Layer(0), "an output showing one layer keeps showing it");
    assert_eq!(p.layer_order, 1);
    assert_eq!(p.decks[0].clip(0, 0).unwrap().name, "top", "the old top layer is now Layer 1");
    assert_eq!(p.decks[0].clip(3, 0).unwrap().name, "bottom");
    assert_eq!(p.composition.layers[0].name, "Titles", "custom names kept");
    assert_eq!(p.composition.layers[1].name, "Layer 2");
    assert_eq!(p.composition.layers[3].name, "Layer 4", "default names follow the numbers");
    assert_eq!(p.keymap.resolve("A"), Some(&Action::TriggerSlot { layer: 0, col: 0 }));
    assert_eq!(p.keymap.resolve("C"), Some(&Action::ClearLayer(3)));
    assert_eq!(p.keymap.resolve("S"), Some(&Action::TriggerColumn(2)));
    assert_eq!(p.panic_media, Some((0, 0, 0)));
    assert_eq!(p.presentation_layer, Some(2));
    assert!(!p.migrate(), "only once");
}

#[test]
fn a_show_without_the_field_is_old() {
    let p: Project = serde_json::from_str("{}").unwrap();
    assert_eq!(p.layer_order, 0);
    let back: Project = serde_json::from_str(&serde_json::to_string(&Project::new_default()).unwrap()).unwrap();
    assert_eq!(back.layer_order, 1);
}

#[test]
fn old_sequences_become_scene_chains() {
    let mut p = Project::new_default();
    for c in 0..3 {
        p.decks[0].slots[0][c] = Some(Clip::new(PathBuf::from(format!("C:/m/{c}.png"))));
    }
    p.decks[0].clip_mut(0, 2).unwrap().still_secs = Some(3.0);
    p.decks[0].sequences.push(Sequence { layer: 0, cols: vec![0, 1, 2], loop_all: true, still_secs: 8.0 });
    assert!(p.migrate());
    assert!(p.decks[0].sequences.is_empty());
    assert_eq!(p.decks[0].scene_chains, vec![SceneChain { cols: vec![0, 1, 2], looping: true }]);
    assert_eq!(p.decks[0].clip(0, 1).unwrap().image_secs(), 8.0, "the sequence's still time moves into its images");
    assert_eq!(p.decks[0].clip(0, 2).unwrap().image_secs(), 3.0, "a set duration stays");
}

#[test]
fn broken_chains_from_a_file_are_dropped_on_open() {
    let mut p = Project::new_default();
    p.decks[0].slots[0][0] = Some(Clip::new(PathBuf::from("C:/m/a.mov")));
    p.decks[0].layer_chains.push(LayerChain { col: 0, steps: Vec::new(), looping: false });
    p.decks[0].scene_chains.push(SceneChain { cols: vec![0, 7], looping: false });
    p.migrate();
    assert!(p.decks[0].layer_chains.is_empty() && p.decks[0].scene_chains.is_empty());
}
