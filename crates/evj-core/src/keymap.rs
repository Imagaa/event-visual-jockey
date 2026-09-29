//! Keyboard shortcuts, saved with the project. One action per key, one key per action.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    TriggerSlot { layer: usize, col: usize },
    TriggerColumn(usize),
    ClearLayer(usize),
    ClearAll,
    TapTempo,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyMap {
    pub bindings: Vec<(String, Action)>,
}

impl KeyMap {
    pub fn bind(&mut self, key: &str, action: Action) {
        self.bindings.retain(|(k, a)| !k.eq_ignore_ascii_case(key) && *a != action);
        self.bindings.push((key.to_string(), action));
    }

    pub fn unbind(&mut self, key: &str) {
        self.bindings.retain(|(k, _)| !k.eq_ignore_ascii_case(key));
    }

    pub fn resolve(&self, key: &str) -> Option<&Action> {
        self.bindings.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, a)| a)
    }

    /// Rewrites every binding's action; None drops the binding.
    pub fn remap(&mut self, f: impl Fn(&Action) -> Option<Action>) {
        self.bindings = std::mem::take(&mut self.bindings).into_iter().filter_map(|(k, a)| f(&a).map(|a| (k, a))).collect();
    }

    pub fn key_for(&self, action: &Action) -> Option<&str> {
        self.bindings.iter().find(|(_, a)| a == action).map(|(k, _)| k.as_str())
    }
}
