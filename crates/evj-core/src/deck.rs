//! Editing operations on the clip grid.
use crate::model::{Clip, Deck, Layer, Project, Sequence};
use std::path::PathBuf;

impl Deck {
    fn columns(&self) -> usize {
        self.slots.first().map_or(0, Vec::len)
    }

    /// Resizes the grid; clips inside the new bounds are kept.
    pub fn ensure_size(&mut self, layers: usize, columns: usize) {
        let columns = columns.max(self.columns());
        self.slots.resize_with(layers, Vec::new);
        for row in &mut self.slots {
            row.resize(columns, None);
        }
    }

    /// Puts clips for `paths` into `layer`, starting at `col` and moving right (adds columns as needed).
    pub fn place(&mut self, layer: usize, col: usize, paths: &[PathBuf]) -> Vec<(usize, usize)> {
        if layer >= self.slots.len() {
            return Vec::new();
        }
        let need = col + paths.len();
        if need > self.columns() {
            let layers = self.slots.len();
            self.ensure_size(layers, need);
        }
        paths
            .iter()
            .enumerate()
            .map(|(i, p)| {
                self.slots[layer][col + i] = Some(Clip::new(p.clone()));
                (layer, col + i)
            })
            .collect()
    }

    pub fn remove(&mut self, layer: usize, col: usize) {
        if let Some(s) = self.slots.get_mut(layer).and_then(|r| r.get_mut(col)) {
            *s = None;
        }
        self.prune_sequences();
    }

    pub fn swap(&mut self, a: (usize, usize), b: (usize, usize)) {
        let valid = |(l, c): (usize, usize)| l < self.slots.len() && c < self.slots[l].len();
        if !valid(a) || !valid(b) {
            return;
        }
        // A moved slot leaves its sequence's order meaningless: the sequence goes.
        let hit = |s: &Sequence, (l, c): (usize, usize)| s.layer == l && s.cols.contains(&c);
        self.sequences.retain(|s| !hit(s, a) && !hit(s, b));
        let ca = self.slots[a.0][a.1].take();
        let cb = self.slots[b.0][b.1].take();
        self.slots[a.0][a.1] = cb;
        self.slots[b.0][b.1] = ca;
    }

    pub fn clip(&self, layer: usize, col: usize) -> Option<&Clip> {
        self.slots.get(layer)?.get(col)?.as_ref()
    }

    pub fn clip_mut(&mut self, layer: usize, col: usize) -> Option<&mut Clip> {
        self.slots.get_mut(layer)?.get_mut(col)?.as_mut()
    }

    pub fn sequence_at(&self, layer: usize, col: usize) -> Option<(usize, &Sequence)> {
        self.sequences.iter().enumerate().find(|(_, s)| s.layer == layer && s.cols.contains(&col))
    }

    /// Filled slots of `layer` become a sequence (column order); they leave any other sequence.
    pub fn make_sequence(&mut self, layer: usize, cols: &[usize]) -> bool {
        let mut cols: Vec<usize> = cols.iter().copied().filter(|&c| self.clip(layer, c).is_some()).collect();
        cols.sort_unstable();
        cols.dedup();
        if cols.len() < 2 {
            return false;
        }
        for s in self.sequences.iter_mut().filter(|s| s.layer == layer) {
            s.cols.retain(|c| !cols.contains(c));
        }
        self.sequences.push(Sequence { layer, cols, ..Sequence::default() });
        self.prune_sequences();
        true
    }

    pub fn break_sequence(&mut self, i: usize) {
        if i < self.sequences.len() {
            self.sequences.remove(i);
        }
    }

    /// Drops slots that are empty now and sequences with fewer than two slots.
    fn prune_sequences(&mut self) {
        let slots = &self.slots;
        let filled = |l: usize, c: usize| slots.get(l).and_then(|r| r.get(c)).is_some_and(Option::is_some);
        for s in &mut self.sequences {
            let l = s.layer;
            s.cols.retain(|&c| filled(l, c));
        }
        self.sequences.retain(|s| s.cols.len() >= 2);
    }

    pub fn scene_name(&self, col: usize) -> String {
        self.scene_names.get(col).map(|n| n.trim()).filter(|n| !n.is_empty()).map_or_else(|| format!("Scene {}", col + 1), str::to_string)
    }

    pub fn set_scene_name(&mut self, col: usize, name: &str) {
        if self.scene_names.len() <= col {
            self.scene_names.resize(col + 1, String::new());
        }
        self.scene_names[col] = name.trim().to_string();
    }
}

impl Project {
    pub fn columns(&self) -> usize {
        self.decks.iter().map(Deck::columns).max().unwrap_or(0)
    }

    pub fn add_layer(&mut self) {
        let n = self.composition.layers.len() + 1;
        self.composition.layers.push(Layer { name: format!("Layer {n}"), ..Layer::default() });
        let cols = self.columns();
        for d in &mut self.decks {
            d.ensure_size(n, cols);
        }
    }

    /// Removes a layer and its clips in every deck; at least one layer always stays.
    pub fn remove_layer(&mut self, layer: usize) {
        if self.composition.layers.len() <= 1 || layer >= self.composition.layers.len() {
            return;
        }
        self.composition.layers.remove(layer);
        for d in &mut self.decks {
            if layer < d.slots.len() {
                d.slots.remove(layer);
            }
            d.sequences.retain(|s| s.layer != layer);
            for s in d.sequences.iter_mut().filter(|s| s.layer > layer) {
                s.layer -= 1;
            }
        }
    }

    pub fn add_column(&mut self) {
        let (layers, cols) = (self.composition.layers.len(), self.columns() + 1);
        for d in &mut self.decks {
            d.ensure_size(layers, cols);
        }
    }

    pub fn add_deck(&mut self) {
        let mut d = Deck { name: format!("Deck {}", self.decks.len() + 1), ..Default::default() };
        d.ensure_size(self.composition.layers.len(), self.columns().max(1));
        self.decks.push(d);
    }

    pub fn deck(&self) -> Option<&Deck> {
        self.decks.get(self.active_deck)
    }

    pub fn deck_mut(&mut self) -> Option<&mut Deck> {
        self.decks.get_mut(self.active_deck)
    }
}
