use evj_core::keymap;
use evj_core::model::*;
use std::path::PathBuf;

fn paths(n: usize) -> Vec<PathBuf> {
    (0..n).map(|i| PathBuf::from(format!("C:/m/clip{i}.mov"))).collect()
}

#[test]
fn place_fills_to_the_right_and_grows_columns() {
    let mut p = Project::new_default();
    let placed = p.decks[0].place(1, 6, &paths(3));
    assert_eq!(placed, vec![(1, 6), (1, 7), (1, 8)]);
    assert!(p.decks[0].slots.iter().all(|row| row.len() == 9), "all rows grow together");
    assert_eq!(p.decks[0].slots[1][8].as_ref().unwrap().name, "clip2");
}

#[test]
fn place_overwrites_occupied_slots() {
    let mut p = Project::new_default();
    p.decks[0].place(0, 0, &paths(1));
    p.decks[0].place(0, 0, &[PathBuf::from("C:/m/other.mp4")]);
    assert_eq!(p.decks[0].slots[0][0].as_ref().unwrap().name, "other");
}

#[test]
fn swap_and_remove() {
    let mut d = Project::new_default().decks.remove(0);
    d.place(0, 0, &paths(1));
    d.swap((0, 0), (3, 5));
    assert!(d.slots[0][0].is_none());
    assert_eq!(d.slots[3][5].as_ref().unwrap().name, "clip0");
    d.remove(3, 5);
    assert!(d.slots[3][5].is_none());
    d.remove(99, 99); // out of range is a no-op
    d.swap((0, 0), (99, 0));
}

#[test]
fn layers_and_columns_can_be_added_and_removed() {
    let mut p = Project::new_default();
    p.decks[0].place(3, 0, &paths(1));
    p.add_layer();
    assert_eq!(p.composition.layers.len(), 5);
    assert!(p.decks.iter().all(|d| d.slots.len() == 5));
    p.add_column();
    assert!(p.decks[0].slots.iter().all(|r| r.len() == 9));
    p.remove_layer(4);
    p.remove_layer(3);
    assert_eq!(p.composition.layers.len(), 3);
    assert!(p.decks[0].slots.iter().flatten().all(|s| s.is_none()), "layer 4 clip went with it");
    for _ in 0..10 {
        p.remove_layer(0);
    }
    assert_eq!(p.composition.layers.len(), 1, "always keep one layer");
}

#[test]
fn new_deck_matches_layer_count() {
    let mut p = Project::new_default();
    p.add_layer();
    p.add_deck();
    assert_eq!(p.decks.len(), 2);
    assert_eq!(p.decks[1].slots.len(), 5);
    assert_eq!(p.decks[1].name, "Deck 2");
}

fn filled(cols: usize) -> Deck {
    let mut d = Deck { name: "D".into(), slots: vec![vec![None; cols]; 2], ..Default::default() };
    for c in 0..cols {
        d.slots[0][c] = Some(Clip::new(PathBuf::from(format!("C:/m/{c}.mov"))));
    }
    d
}

#[test]
fn layer_chains_live_in_one_column() {
    let mut p = Project::new_default();
    for l in 0..3 {
        p.decks[0].slots[l][2] = Some(Clip::new(PathBuf::from(format!("C:/m/{l}.mov"))));
    }
    let d = &mut p.decks[0];
    assert!(!d.make_layer_chain(2, &[0]), "one layer is not a chain");
    assert!(!d.make_layer_chain(2, &[0, 3]), "layer 4 is empty in that column");
    assert!(d.make_layer_chain(2, &[2, 0, 1]));
    assert_eq!(d.layer_chains[0].steps.iter().map(|s| s.layer).collect::<Vec<_>>(), vec![0, 1, 2]);
    assert_eq!(d.layer_chain_at(1, 2).map(|(i, _)| i), Some(0));
    assert!(d.layer_chain_at(1, 3).is_none());
    assert!(d.make_layer_chain(2, &[1, 2]), "a layer belongs to one chain");
    assert_eq!(d.layer_chains.len(), 1, "the old chain kept only layer 1 and went");
    d.remove(1, 2);
    assert!(d.layer_chains.is_empty(), "one member left: no chain");
}

#[test]
fn scene_chains_and_removing_a_scene() {
    let mut p = Project::new_default();
    for c in 0..4 {
        p.decks[0].slots[0][c] = Some(Clip::new(PathBuf::from(format!("C:/m/{c}.mov"))));
    }
    p.decks[0].slots[1][3] = Some(Clip::new(PathBuf::from("C:/m/x.mov")));
    p.decks[0].slots[2][3] = Some(Clip::new(PathBuf::from("C:/m/y.mov")));
    assert!(!p.decks[0].make_scene_chain(&[1, 7]), "scene 8 is empty");
    assert!(p.decks[0].make_scene_chain(&[3, 1, 2]));
    assert!(p.decks[0].make_layer_chain(3, &[1, 2]));
    assert_eq!(p.decks[0].scene_chains[0].cols, vec![1, 2, 3]);
    assert_eq!(p.decks[0].scene_chain_at(2).map(|(i, _)| i), Some(0));
    p.decks[0].set_scene_name(3, "End");
    p.keymap.bind("Z", keymap::Action::TriggerColumn(3));
    p.keymap.bind("Y", keymap::Action::TriggerSlot { layer: 0, col: 2 });
    p.panic_media = Some((0, 0, 2));
    let cols = p.columns();
    p.remove_column(2);
    assert_eq!(p.columns(), cols - 1);
    assert_eq!(p.decks[0].scene_chains[0].cols, vec![1, 2], "column 4 moved to 3");
    assert_eq!(p.decks[0].layer_chains[0].col, 2);
    assert_eq!(p.decks[0].scene_name(2), "End");
    assert_eq!(p.keymap.resolve("Z"), Some(&keymap::Action::TriggerColumn(2)));
    assert_eq!(p.keymap.resolve("Y"), None, "its slot is gone");
    assert_eq!(p.panic_media, None);
    p.remove_column(1);
    assert!(p.decks[0].scene_chains.is_empty(), "one scene left: no chain");
}

#[test]
fn moving_a_layer_moves_its_slots_chains_and_keys() {
    let mut p = Project::new_default();
    p.decks[0].slots[0][0] = Some(Clip::new(PathBuf::from("C:/m/a.mov")));
    p.decks[0].slots[1][0] = Some(Clip::new(PathBuf::from("C:/m/b.mov")));
    p.decks[0].make_layer_chain(0, &[0, 1]);
    p.decks[0].layer_chains[0].steps[1].mode = StepMode::Overlay;
    p.composition.layers[0].opacity = 0.5;
    p.keymap.bind("A", keymap::Action::ClearLayer(0));
    p.panic_media = Some((0, 1, 0));
    p.move_layer(0, 1);
    assert_eq!(p.decks[0].clip(1, 0).unwrap().name, "a");
    assert_eq!(p.composition.layers[1].opacity, 0.5);
    let steps = &p.decks[0].layer_chains[0].steps;
    assert_eq!(steps.iter().map(|s| s.layer).collect::<Vec<_>>(), vec![0, 1], "steps re-sorted by layer");
    assert_eq!(steps[1].mode, StepMode::Replace, "a step's settings move with its layer");
    assert_eq!(p.keymap.resolve("A"), Some(&keymap::Action::ClearLayer(1)));
    assert_eq!(p.panic_media, Some((0, 0, 0)));
}

#[test]
fn removing_a_layer_keeps_chains_valid() {
    let mut p = Project::new_default();
    for l in 0..3 {
        p.decks[0].slots[l][0] = Some(Clip::new(PathBuf::from(format!("C:/m/{l}.mov"))));
    }
    p.decks[0].make_layer_chain(0, &[0, 1, 2]);
    p.remove_layer(0);
    assert_eq!(p.decks[0].layer_chains[0].steps.iter().map(|s| s.layer).collect::<Vec<_>>(), vec![0, 1]);
    p.remove_layer(0);
    assert!(p.decks[0].layer_chains.is_empty());
}

#[test]
fn swapping_a_chain_slot_breaks_that_chain() {
    let mut p = Project::new_default();
    p.decks[0] = filled(3);
    p.decks[0].slots[1][0] = Some(Clip::new(PathBuf::from("C:/m/x.mov")));
    p.decks[0].make_layer_chain(0, &[0, 1]);
    p.decks[0].make_scene_chain(&[1, 2]);
    p.decks[0].swap((0, 0), (0, 1));
    assert!(p.decks[0].layer_chains.is_empty());
    assert_eq!(p.decks[0].scene_chains.len(), 1, "scenes keep their order");
}

#[test]
fn scenes_have_default_and_custom_names() {
    let mut d = filled(3);
    assert_eq!(d.scene_name(0), "Scene 1");
    d.set_scene_name(2, "Opening");
    assert_eq!(d.scene_name(2), "Opening");
    d.set_scene_name(2, "  ");
    assert_eq!(d.scene_name(2), "Scene 3");
}
