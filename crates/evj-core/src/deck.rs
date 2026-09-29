//! Editing operations on the clip grid.
use crate::model::{ChainStep, Clip, Deck, Layer, LayerChain, Project, SceneChain};
use crate::keymap::Action;
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
        self.prune_chains();
    }

    pub fn swap(&mut self, a: (usize, usize), b: (usize, usize)) {
        let valid = |(l, c): (usize, usize)| l < self.slots.len() && c < self.slots[l].len();
        if !valid(a) || !valid(b) {
            return;
        }
        // A moved slot leaves its layer chain's order meaningless: the chain goes.
        let hit = |ch: &LayerChain, (l, c): (usize, usize)| ch.col == c && ch.steps.iter().any(|s| s.layer == l);
        self.layer_chains.retain(|ch| !hit(ch, a) && !hit(ch, b));
        let ca = self.slots[a.0][a.1].take();
        let cb = self.slots[b.0][b.1].take();
        self.slots[a.0][a.1] = cb;
        self.slots[b.0][b.1] = ca;
        self.prune_chains();
    }

    pub fn clip(&self, layer: usize, col: usize) -> Option<&Clip> {
        self.slots.get(layer)?.get(col)?.as_ref()
    }

    pub fn clip_mut(&mut self, layer: usize, col: usize) -> Option<&mut Clip> {
        self.slots.get_mut(layer)?.get_mut(col)?.as_mut()
    }

    pub fn layer_chain_at(&self, layer: usize, col: usize) -> Option<(usize, &LayerChain)> {
        self.layer_chains.iter().enumerate().find(|(_, ch)| ch.col == col && ch.steps.iter().any(|s| s.layer == layer))
    }

    pub fn scene_chain_at(&self, col: usize) -> Option<(usize, &SceneChain)> {
        self.scene_chains.iter().enumerate().find(|(_, ch)| ch.cols.contains(&col))
    }

    fn scene_filled(&self, col: usize) -> bool {
        self.slots.iter().any(|r| r.get(col).is_some_and(Option::is_some))
    }

    /// Filled slots of `layers` in column `col` start one after another (layer order);
    /// they leave any other chain of that column.
    pub fn make_layer_chain(&mut self, col: usize, layers: &[usize]) -> bool {
        let mut layers: Vec<usize> = layers.iter().copied().filter(|&l| self.clip(l, col).is_some()).collect();
        layers.sort_unstable();
        layers.dedup();
        if layers.len() < 2 {
            return false;
        }
        for ch in self.layer_chains.iter_mut().filter(|ch| ch.col == col) {
            ch.steps.retain(|s| !layers.contains(&s.layer));
        }
        let steps = layers.into_iter().map(|layer| ChainStep { layer, ..ChainStep::default() }).collect();
        self.layer_chains.push(LayerChain { col, steps, looping: false });
        self.prune_chains();
        true
    }

    /// Scenes with at least one clip play one after another (column order).
    pub fn make_scene_chain(&mut self, cols: &[usize]) -> bool {
        let mut cols: Vec<usize> = cols.iter().copied().filter(|&c| self.scene_filled(c)).collect();
        cols.sort_unstable();
        cols.dedup();
        if cols.len() < 2 {
            return false;
        }
        for ch in &mut self.scene_chains {
            ch.cols.retain(|c| !cols.contains(c));
        }
        self.scene_chains.push(SceneChain { cols, looping: false });
        self.prune_chains();
        true
    }

    pub fn break_layer_chain(&mut self, i: usize) {
        if i < self.layer_chains.len() {
            self.layer_chains.remove(i);
        }
    }

    pub fn break_scene_chain(&mut self, i: usize) {
        if i < self.scene_chains.len() {
            self.scene_chains.remove(i);
        }
    }

    /// Drops members that are empty now and chains with fewer than two members.
    pub(crate) fn prune_chains(&mut self) {
        let slots = &self.slots;
        let filled = |l: usize, c: usize| slots.get(l).and_then(|r| r.get(c)).is_some_and(Option::is_some);
        for ch in &mut self.layer_chains {
            let c = ch.col;
            ch.steps.retain(|s| filled(s.layer, c));
            ch.steps.sort_by_key(|s| s.layer);
            ch.steps.dedup_by_key(|s| s.layer);
        }
        self.layer_chains.retain(|ch| ch.steps.len() >= 2);
        for ch in &mut self.scene_chains {
            ch.cols.retain(|&c| slots.iter().any(|r| r.get(c).is_some_and(Option::is_some)));
        }
        self.scene_chains.retain(|ch| ch.cols.len() >= 2);
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
        for d in &mut self.decks {
            for ch in &mut d.layer_chains {
                ch.steps.retain(|s| s.layer != layer);
            }
        }
        self.keymap.remap(|a| match a {
            Action::TriggerSlot { layer: l, .. } | Action::ClearLayer(l) if *l == layer => None,
            a => Some(a.clone()),
        });
        if self.panic_media.is_some_and(|(_, l, _)| l == layer) {
            self.panic_media = None;
        }
        self.remap_layers(|l| if l > layer { l - 1 } else { l });
        self.composition.layers.remove(layer);
        for d in &mut self.decks {
            if layer < d.slots.len() {
                d.slots.remove(layer);
            }
            d.prune_chains();
        }
    }

    /// Swaps two layers with their slots in every deck (clips on air follow via the engine's MoveLayer).
    pub fn move_layer(&mut self, from: usize, to: usize) {
        let n = self.composition.layers.len();
        if from >= n || to >= n || from == to {
            return;
        }
        self.remap_layers(|l| if l == from { to } else if l == to { from } else { l });
        self.composition.layers.swap(from, to);
        let cols = self.columns();
        for d in &mut self.decks {
            d.ensure_size(n, cols);
            d.slots.swap(from, to);
        }
    }

    /// Removes a scene (column) in every deck; at least one column always stays.
    pub fn remove_column(&mut self, col: usize) {
        if self.columns() <= 1 || col >= self.columns() {
            return;
        }
        let shift = |c: usize| (c != col).then(|| if c > col { c - 1 } else { c });
        for d in &mut self.decks {
            for row in &mut d.slots {
                if col < row.len() {
                    row.remove(col);
                }
            }
            if col < d.scene_names.len() {
                d.scene_names.remove(col);
            }
            d.layer_chains.retain(|ch| ch.col != col);
            for ch in &mut d.layer_chains {
                ch.col = shift(ch.col).unwrap_or(ch.col);
            }
            for ch in &mut d.scene_chains {
                ch.cols = ch.cols.iter().filter_map(|&c| shift(c)).collect();
            }
            d.prune_chains();
        }
        self.keymap.remap(|a| match *a {
            Action::TriggerSlot { layer, col } => shift(col).map(|col| Action::TriggerSlot { layer, col }),
            Action::TriggerColumn(c) => shift(c).map(Action::TriggerColumn),
            ref a => Some(a.clone()),
        });
        self.panic_media = self.panic_media.and_then(|(d, l, c)| shift(c).map(|c| (d, l, c)));
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
