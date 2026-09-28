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
fn sequences_need_two_filled_slots_in_one_layer_and_are_ordered() {
    let mut d = filled(4);
    assert!(!d.make_sequence(0, &[1]), "one slot is not a sequence");
    assert!(!d.make_sequence(1, &[0, 1]), "empty slots do not count");
    assert!(d.make_sequence(0, &[3, 1, 2]));
    assert_eq!(d.sequences[0].cols, vec![1, 2, 3]);
    assert_eq!(d.sequence_at(0, 2).map(|(i, _)| i), Some(0));
    assert!(d.sequence_at(0, 0).is_none());
}

#[test]
fn a_slot_belongs_to_one_sequence() {
    let mut d = filled(4);
    assert!(d.make_sequence(0, &[0, 1, 2]));
    assert!(d.make_sequence(0, &[2, 3]));
    assert_eq!(d.sequences.len(), 2);
    assert_eq!(d.sequences[0].cols, vec![0, 1]);
    assert_eq!(d.sequences[1].cols, vec![2, 3]);
}

#[test]
fn removing_slots_and_layers_keeps_sequences_valid() {
    let mut d = filled(3);
    d.make_sequence(0, &[0, 1, 2]);
    d.remove(0, 1);
    assert_eq!(d.sequences[0].cols, vec![0, 2]);
    d.remove(0, 0);
    assert!(d.sequences.is_empty(), "a sequence with one slot left is gone");
    let mut p = Project::new_default();
    p.decks[0] = filled(3);
    p.decks[0].slots.resize(p.composition.layers.len(), vec![None; 3]);
    p.decks[0].make_sequence(0, &[0, 1]);
    p.decks[0].slots[2][0] = Some(Clip::new(PathBuf::from("C:/m/x.mov")));
    p.decks[0].slots[2][1] = Some(Clip::new(PathBuf::from("C:/m/y.mov")));
    p.decks[0].make_sequence(2, &[0, 1]);
    p.remove_layer(0);
    assert_eq!(p.decks[0].sequences.len(), 1);
    assert_eq!(p.decks[0].sequences[0].layer, 1, "the layer above moved down");
}

#[test]
fn swapping_a_sequence_slot_breaks_that_sequence() {
    let mut d = filled(3);
    d.make_sequence(0, &[0, 1]);
    d.swap((0, 1), (0, 2));
    assert!(d.sequences.is_empty());
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
