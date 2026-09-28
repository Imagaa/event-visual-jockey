use evj_core::keymap::{Action, KeyMap};
use evj_core::model::Project;

#[test]
fn bind_resolve_and_rebind() {
    let mut k = KeyMap::default();
    k.bind("Q", Action::TriggerSlot { layer: 0, col: 0 });
    k.bind("W", Action::TriggerColumn(2));
    assert_eq!(k.resolve("Q"), Some(&Action::TriggerSlot { layer: 0, col: 0 }));
    assert_eq!(k.resolve("q"), Some(&Action::TriggerSlot { layer: 0, col: 0 }), "case-insensitive");
    assert_eq!(k.key_for(&Action::TriggerColumn(2)), Some("W"));

    // A key does one thing; an action has one key.
    k.bind("Q", Action::ClearAll);
    assert_eq!(k.resolve("Q"), Some(&Action::ClearAll));
    assert_eq!(k.key_for(&Action::TriggerSlot { layer: 0, col: 0 }), None);
    k.bind("E", Action::ClearAll);
    assert_eq!(k.resolve("Q"), None);
    assert_eq!(k.key_for(&Action::ClearAll), Some("E"));

    k.unbind("E");
    assert_eq!(k.resolve("E"), None);
}

#[test]
fn keymap_is_saved_with_the_project() {
    let mut p = Project::new_default();
    p.keymap.bind("F1", Action::ClearLayer(3));
    let back: Project = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
    assert_eq!(back.keymap.resolve("F1"), Some(&Action::ClearLayer(3)));
}
