//! Chains: layers of one scene that start one after another (a layer chain), and scenes that
//! play one after another (a scene chain). The app runs them from the snapshot and the UI clock.
use crate::ui::{Actions, UiState};
use evj_core::model::{Clip, LayerChain, PlayMode, SceneChain, StepMode, StepStart};
use evj_engine::{LayerState, Snapshot};
use std::path::PathBuf;

/// Seconds a clip gets to open before a failure stops its chain (older snapshots may still
/// show the error of the clip before).
const OPEN_GRACE: f64 = 0.5;

/// A clip inside a chain plays once (an image for its duration); an A–B loop (Loop with a start /
/// end set) keeps looping until Next — plain clips default to Loop, so that alone does not count.
pub fn play_clip(clip: &Clip) -> Clip {
    let mut c = clip.clone();
    let ab_loop = c.mode == PlayMode::Loop && (c.in_point > 0.0 || c.out_point < 1.0);
    if !ab_loop {
        c.mode = PlayMode::Once;
    }
    c
}

/// One clip a run waits for. `finished` only counts after the clip was seen playing: the
/// snapshot can still show the finished clip that played before it.
#[derive(Clone, Debug, PartialEq)]
struct Watch {
    layer: usize,
    path: PathBuf,
    armed: bool,
}

impl Watch {
    fn new(layer: usize, path: PathBuf) -> Watch {
        Watch { layer, path, armed: false }
    }

    fn ours<'a>(&self, snap: &'a Snapshot) -> Option<&'a LayerState> {
        snap.layers.get(self.layer).filter(|s| s.clip_path.as_ref() == Some(&self.path))
    }

    fn ended(&mut self, snap: &Snapshot) -> bool {
        match self.ours(snap) {
            Some(s) if !s.finished && !s.loading => {
                self.armed = true;
                false
            }
            Some(s) => self.armed && s.finished,
            None => false,
        }
    }

    /// The clip could not open: the layer reports an error and never showed it.
    fn failed(&self, snap: &Snapshot, since: f64) -> Option<String> {
        let s = snap.layers.get(self.layer)?;
        (!self.armed && since > OPEN_GRACE && !s.loading && self.ours(snap).is_none()).then(|| s.error.clone()).flatten()
    }
}

/// A layer chain playing.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerRun {
    pub deck: usize,
    /// The chain as it was started: an edited chain ends the run.
    pub chain: LayerChain,
    /// The step started last.
    pub step: usize,
    started_at: f64,
    watch: Watch,
}

/// A scene chain playing.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneRun {
    pub deck: usize,
    pub chain: SceneChain,
    /// Position in `chain.cols` of the scene on air.
    pub idx: usize,
    started_at: f64,
    /// Layers of the scene outside a layer chain.
    watches: Vec<Watch>,
}

/// The slot is played by a running chain (and so plays once).
pub fn in_run(st: &UiState, layer: usize, col: usize) -> bool {
    let deck = st.project.active_deck;
    let layer_run = st.layer_runs.iter().any(|r| r.deck == deck && r.chain.col == col && r.chain.steps.iter().any(|s| s.layer == layer));
    let scene_run = st.scene_run.as_ref().is_some_and(|r| r.deck == deck && r.chain.cols.get(r.idx) == Some(&col));
    layer_run || scene_run
}

/// A settings change of a slot on air: inside a running chain it keeps playing as the chain
/// plays it, or the chain would stall.
pub fn as_played(st: &UiState, layer: usize, col: usize, clip: Clip) -> Clip {
    if in_run(st, layer, col) { play_clip(&clip) } else { clip }
}

/// A slot of `deck` played by a running chain plays as the chain plays it.
pub fn as_played_on(st: &UiState, deck: usize, layer: usize, col: usize, clip: Clip) -> Clip {
    let layer_run = st.layer_runs.iter().any(|r| r.deck == deck && r.chain.col == col && r.chain.steps.iter().any(|s| s.layer == layer));
    let scene_run = st.scene_run.as_ref().is_some_and(|r| r.deck == deck && r.chain.cols.get(r.idx) == Some(&col));
    if layer_run || scene_run { play_clip(&clip) } else { clip }
}

/// Plays step `i` of a layer run; a Replace step clears the step before it.
fn play_step(st: &mut UiState, run: &mut LayerRun, i: usize, act: &mut Actions) {
    let step = run.chain.steps[i];
    let Some(clip) = st.project.decks.get(run.deck).and_then(|d| d.clip(step.layer, run.chain.col)).cloned() else { return };
    if i > 0 && step.mode == StepMode::Replace {
        st.off(run.chain.steps[i - 1].layer, act);
    }
    run.step = i;
    run.started_at = st.now;
    run.watch = Watch::new(step.layer, clip.path.clone());
    st.play(run.deck, step.layer, run.chain.col, play_clip(&clip), act);
}

/// Starts the layer chain of `deck` that holds (`layer`, `col`) at that layer's step; the
/// chain's other layers are cleared. false when the slot is in no chain.
pub fn start_layer_chain(st: &mut UiState, deck: usize, col: usize, layer: usize, act: &mut Actions) -> bool {
    let Some(chain) = st.project.decks.get(deck).and_then(|d| d.layer_chain_at(layer, col)).map(|(_, ch)| ch.clone()) else { return false };
    let at = chain.steps.iter().position(|s| s.layer == layer).unwrap_or(0);
    for s in &chain.steps {
        stop_layer_runs(st, s.layer);
        if s.layer != layer && st.playing.get(s.layer).is_some_and(Option::is_some) {
            st.off(s.layer, act);
        }
    }
    let mut run = LayerRun { deck, chain, step: at, started_at: st.now, watch: Watch::new(layer, PathBuf::new()) };
    play_step(st, &mut run, at, act);
    st.layer_runs.push(run);
    true
}

/// The layers of a scene outside its layer chains: the clips the scene waits for.
fn scene_watches(st: &UiState, deck: usize, col: usize) -> Vec<Watch> {
    let Some(d) = st.project.decks.get(deck) else { return Vec::new() };
    (0..st.project.composition.layers.len()).filter(|&l| d.layer_chain_at(l, col).is_none()).filter_map(|l| Some(Watch::new(l, d.clip(l, col)?.path.clone()))).collect()
}

/// Puts scene `idx` of the scene run on air.
fn show_step(st: &mut UiState, idx: usize, act: &mut Actions) {
    let Some(run) = st.scene_run.as_mut() else { return };
    run.idx = idx;
    run.started_at = st.now;
    let (deck, col) = (run.deck, run.chain.cols[idx]);
    st.show_scene(deck, col, act);
    let watches = scene_watches(st, deck, col);
    if let Some(run) = st.scene_run.as_mut() {
        run.watches = watches;
    }
}

/// Starts the scene chain holding column `col` (active deck) there. false when it is in none.
pub fn start_scene_chain(st: &mut UiState, col: usize, act: &mut Actions) -> bool {
    let deck = st.project.active_deck;
    let Some(chain) = st.project.decks.get(deck).and_then(|d| d.scene_chain_at(col)).map(|(_, ch)| ch.clone()) else { return false };
    let idx = chain.cols.iter().position(|&c| c == col).unwrap_or(0);
    st.scene_run = Some(SceneRun { deck, chain, idx, started_at: st.now, watches: Vec::new() });
    show_step(st, idx, act);
    true
}

/// Layer chains running on `layer` stop (the picture stays).
pub fn stop_layer_runs(st: &mut UiState, layer: usize) {
    st.layer_runs.retain(|r| !r.chain.steps.iter().any(|s| s.layer == layer));
}

/// A trigger / clear by hand on `layer`: its layer chains stop, and the scene chain too when
/// the layer is part of the scene on air.
pub fn stop_layer(st: &mut UiState, layer: usize) {
    stop_layer_runs(st, layer);
    let in_scene = st.scene_run.as_ref().is_some_and(|r| st.project.decks.get(r.deck).and_then(|d| d.clip(layer, r.chain.cols[r.idx])).is_some());
    if in_scene {
        st.scene_run = None;
    }
}

/// One layer run for one frame; false = the run is over.
fn step_layer_run(st: &mut UiState, run: &mut LayerRun, snap: &Snapshot, act: &mut Actions) -> bool {
    if !st.project.decks.get(run.deck).is_some_and(|d| d.layer_chains.contains(&run.chain)) {
        return false; // edited: a new chain needs a new start
    }
    if let Some(why) = run.watch.failed(snap, st.now - run.started_at) {
        st.status = format!("Layer chain stopped: Layer {} could not open its clip. {why}", run.watch.layer + 1);
        return false;
    }
    let ended = run.watch.ended(snap);
    match run.chain.steps.get(run.step + 1).copied() {
        Some(next) => {
            let due = match next.start {
                StepStart::AfterSecs(n) => st.now - run.started_at >= n,
                StepStart::AfterPrevious => ended,
            };
            if due {
                let i = run.step + 1;
                play_step(st, run, i, act);
            }
            true
        }
        None if ended && run.chain.looping => {
            for s in run.chain.steps.clone() {
                st.off(s.layer, act);
            }
            play_step(st, run, 0, act);
            true
        }
        None => !ended,
    }
}

/// The scene run for one frame: the scene is done when every clip of it ended (A–B loops never
/// do) and its layer chains are over.
fn step_scene_run(st: &mut UiState, snap: &Snapshot, act: &mut Actions) {
    let Some(mut run) = st.scene_run.clone() else { return };
    if !st.project.decks.get(run.deck).is_some_and(|d| d.scene_chains.contains(&run.chain)) {
        st.scene_run = None;
        return;
    }
    let since = st.now - run.started_at;
    if let Some((layer, why)) = run.watches.iter().find_map(|w| Some((w.layer, w.failed(snap, since)?))) {
        st.status = format!("Scene chain stopped: Layer {} could not open its clip. {why}", layer + 1);
        st.scene_run = None;
        return;
    }
    // Every watch is asked each frame, so each one arms as soon as its clip plays.
    let clips_done = run.watches.iter_mut().fold(true, |all, w| w.ended(snap) && all);
    let col = run.chain.cols[run.idx];
    let chains_done = !st.layer_runs.iter().any(|r| r.deck == run.deck && r.chain.col == col);
    st.scene_run = Some(run);
    if clips_done && chains_done {
        advance_scene(st, act);
    }
}

/// The scene run's next scene (or the first again when it loops); false at its end.
fn advance_scene(st: &mut UiState, act: &mut Actions) -> bool {
    let Some(run) = &st.scene_run else { return false };
    let next = if run.idx + 1 < run.chain.cols.len() {
        run.idx + 1
    } else if run.chain.looping {
        0
    } else {
        st.scene_run = None;
        return false;
    };
    show_step(st, next, act);
    true
}

/// Every frame: the runs move on when a clip ends or a step's time is up.
pub fn update(st: &mut UiState, snap: &Snapshot, now: f64, act: &mut Actions) {
    st.now = now;
    let mut runs = std::mem::take(&mut st.layer_runs);
    runs.retain_mut(|run| step_layer_run(st, run, snap, act));
    runs.append(&mut st.layer_runs);
    st.layer_runs = runs;
    step_scene_run(st, snap, act);
}

/// ⏭ on the scene chain: the next scene now.
pub fn next_scene(st: &mut UiState, act: &mut Actions) {
    if st.scene_run.is_some() && !advance_scene(st, act) {
        st.status = "End of the scene chain".into();
    }
}

/// ⏭ on layer run `k`: its next step now (the first again when it loops).
pub fn next_layer_run(st: &mut UiState, k: usize, act: &mut Actions) {
    let Some(mut run) = st.layer_runs.get(k).cloned() else { return };
    if run.step + 1 < run.chain.steps.len() {
        let i = run.step + 1;
        play_step(st, &mut run, i, act);
    } else if run.chain.looping {
        for s in run.chain.steps.clone() {
            st.off(s.layer, act);
        }
        play_step(st, &mut run, 0, act);
    } else {
        st.status = "End of the layer chain".into();
        return;
    }
    if let Some(r) = st.layer_runs.get_mut(k) {
        *r = run;
    }
}

/// Next (N): the layer chain of the selected layer, else the scene chain, else the first layer chain.
pub fn next_now(st: &mut UiState, act: &mut Actions) {
    let selected = match st.selected {
        crate::ui::Selection::Slot(l, _) | crate::ui::Selection::Layer(l) => Some(l),
        _ => None,
    };
    let of_selected = selected.and_then(|l| st.layer_runs.iter().position(|r| r.chain.steps.iter().any(|s| s.layer == l)));
    match of_selected {
        Some(k) => next_layer_run(st, k, act),
        None if st.scene_run.is_some() => next_scene(st, act),
        None if !st.layer_runs.is_empty() => next_layer_run(st, 0, act),
        None => st.status = "No chain is playing".into(),
    }
}

/// Shift+click on slots: add / remove one (any scene).
pub fn toggle_multi(st: &mut UiState, layer: usize, col: usize) {
    match st.multi.iter().position(|s| *s == (layer, col)) {
        Some(i) => {
            st.multi.remove(i);
        }
        None => {
            st.multi.push((layer, col));
            st.multi.sort_unstable();
        }
    }
    st.status = pick_status(st);
}

/// What the Shift+clicked slots can become.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pick {
    /// All in one scene: (column, layers) — a layer chain.
    Layers(usize, Vec<usize>),
    /// Spread over scenes: their columns — a scene chain.
    Scenes(Vec<usize>),
}

pub fn pick(st: &UiState) -> Option<Pick> {
    if st.multi.len() < 2 {
        return None;
    }
    let mut cols: Vec<usize> = st.multi.iter().map(|(_, c)| *c).collect();
    cols.sort_unstable();
    cols.dedup();
    Some(if cols.len() == 1 { Pick::Layers(cols[0], st.multi.iter().map(|(l, _)| *l).collect()) } else { Pick::Scenes(cols) })
}

/// Makes the chain the selection stands for; true when one was made.
pub fn apply_pick(st: &mut UiState) -> bool {
    let made = match (pick(st), st.project.deck_mut()) {
        (Some(Pick::Layers(col, layers)), Some(d)) => d.make_layer_chain(col, &layers),
        (Some(Pick::Scenes(cols)), Some(d)) => d.make_scene_chain(&cols),
        _ => false,
    };
    if made {
        st.status = "Chained — play its first clip / scene; timing and Stop / Loop: Preview › Chain".into();
        st.dirty = true;
    }
    st.multi.clear();
    made
}

fn pick_status(st: &UiState) -> String {
    match pick(st) {
        None if st.multi.is_empty() => String::new(),
        None => "1 slot picked — Shift+click more: same scene = layer chain, other scenes = scene chain".into(),
        Some(Pick::Layers(_, l)) => format!("{} slots of one scene picked — right-click › Chain layers", l.len()),
        Some(Pick::Scenes(c)) => format!("{} scenes picked — right-click › Chain scenes", c.len()),
    }
}

/// Shift+click on scene headers.
pub fn toggle_scene(st: &mut UiState, col: usize) {
    match st.multi_scenes.iter().position(|c| *c == col) {
        Some(i) => {
            st.multi_scenes.remove(i);
        }
        None => {
            st.multi_scenes.push(col);
            st.multi_scenes.sort_unstable();
        }
    }
}

/// New settings for layer chain `i` of `deck`; a run of it goes on with them.
pub fn edit_layer_chain(st: &mut UiState, deck: usize, i: usize, chain: LayerChain) {
    let Some(old) = st.project.decks.get_mut(deck).and_then(|d| d.layer_chains.get_mut(i)) else { return };
    for r in st.layer_runs.iter_mut().filter(|r| r.deck == deck && r.chain == *old) {
        r.chain = chain.clone();
    }
    *old = chain;
    st.dirty = true;
}

/// New settings for scene chain `i` of `deck`; a run of it goes on with them.
pub fn edit_scene_chain(st: &mut UiState, deck: usize, i: usize, chain: SceneChain) {
    let Some(old) = st.project.decks.get_mut(deck).and_then(|d| d.scene_chains.get_mut(i)) else { return };
    if let Some(r) = st.scene_run.as_mut().filter(|r| r.deck == deck && r.chain == *old) {
        r.chain = chain.clone();
    }
    *old = chain;
    st.dirty = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use evj_core::model::{ChainStep, Project};
    use evj_engine::Command;

    /// Column 0: a clip on layers 1–3. Columns 1 and 2: a clip on layer 1.
    fn show() -> UiState {
        let mut p = Project::new_default();
        for l in 0..3 {
            p.decks[0].slots[l][0] = Some(Clip::new(format!("C:/m/{l}.mov").into()));
        }
        for c in 1..3 {
            p.decks[0].slots[0][c] = Some(Clip::new(format!("C:/m/s{c}.mov").into()));
        }
        UiState::new(p)
    }

    /// What the engine shows: every layer's clip on air, `finished` ones at their end.
    fn snap(st: &UiState, finished: &[usize]) -> Snapshot {
        let mut s = Snapshot { layers: vec![LayerState::default(); st.playing.len()], ..Default::default() };
        for (l, p) in st.playing.iter().enumerate() {
            if let Some((d, c)) = *p {
                let clip = st.project.decks[d].clip(l, c).unwrap();
                s.layers[l].clip_path = Some(clip.path.clone());
                s.layers[l].clip_name = Some(clip.name.clone());
                s.layers[l].has_frame = true;
                s.layers[l].finished = finished.contains(&l);
            }
        }
        s
    }

    fn triggered(act: &Actions) -> Vec<usize> {
        act.commands.iter().filter_map(|c| if let Command::Trigger { layer, .. } = c { Some(*layer) } else { None }).collect()
    }

    fn cleared(act: &Actions) -> Vec<usize> {
        act.commands.iter().filter_map(|c| if let Command::Clear { layer, .. } = c { Some(*layer) } else { None }).collect()
    }

    /// One frame: the engine shows `finished`, the runs react.
    fn tick(st: &mut UiState, finished: &[usize], now: f64) -> Actions {
        let s = snap(st, finished);
        let mut act = Actions::default();
        update(st, &s, now, &mut act);
        act
    }

    #[test]
    fn a_layer_chain_overlays_after_seconds_and_replaces_after_the_previous() {
        let mut st = show();
        let d = &mut st.project.decks[0];
        assert!(d.make_layer_chain(0, &[0, 1, 2]));
        d.layer_chains[0].steps[1] = ChainStep { layer: 1, start: StepStart::AfterSecs(10.0), mode: StepMode::Overlay };
        d.layer_chains[0].steps[2] = ChainStep { layer: 2, start: StepStart::AfterPrevious, mode: StepMode::Replace };
        let mut act = Actions::default();
        st.trigger(0, 0, &mut act);
        assert_eq!(triggered(&act), vec![0], "the chain starts at layer 1");
        let once = act.commands.iter().any(|c| matches!(c, Command::Trigger { clip, .. } if clip.mode == PlayMode::Once));
        assert!(once, "chain clips play once");
        assert!(triggered(&tick(&mut st, &[], 9.0)).is_empty(), "not yet");
        let act = tick(&mut st, &[], 10.1);
        assert_eq!(triggered(&act), vec![1], "layer 2 after 10 s");
        assert!(cleared(&act).is_empty(), "overlay keeps layer 1");
        tick(&mut st, &[], 11.0);
        let act = tick(&mut st, &[1], 20.0);
        assert_eq!(triggered(&act), vec![2], "layer 3 when layer 2 ends");
        assert_eq!(cleared(&act), vec![1], "replace clears layer 2");
        tick(&mut st, &[], 21.0);
        let act = tick(&mut st, &[2], 30.0);
        assert!(triggered(&act).is_empty() && st.layer_runs.is_empty(), "stop at the end");
        assert_eq!(st.playing[2], Some((0, 0)), "the last picture stays");
    }

    #[test]
    fn a_stale_finished_flag_does_not_count() {
        let mut st = show();
        st.project.decks[0].make_layer_chain(0, &[0, 1]);
        let mut act = Actions::default();
        st.trigger(0, 0, &mut act);
        assert!(triggered(&tick(&mut st, &[0], 1.0)).is_empty(), "never seen playing: the flag is the clip before's");
        tick(&mut st, &[], 2.0);
        assert_eq!(triggered(&tick(&mut st, &[0], 3.0)), vec![1]);
    }

    #[test]
    fn a_looping_layer_chain_clears_and_starts_over() {
        let mut st = show();
        st.project.decks[0].make_layer_chain(0, &[0, 1]);
        st.project.decks[0].layer_chains[0].looping = true;
        let mut act = Actions::default();
        st.trigger(0, 0, &mut act);
        tick(&mut st, &[], 1.0);
        assert_eq!(triggered(&tick(&mut st, &[0], 5.0)), vec![1]);
        tick(&mut st, &[], 6.0);
        let act = tick(&mut st, &[1], 9.0);
        assert!(cleared(&act).contains(&1), "{:?}", cleared(&act));
        assert_eq!(triggered(&act), vec![0], "back to layer 1");
        assert_eq!(st.layer_runs.len(), 1);
    }

    #[test]
    fn a_scene_chain_waits_for_its_longest_clip_then_stops() {
        let mut st = show();
        assert!(st.project.decks[0].make_scene_chain(&[0, 1, 2]));
        let mut act = Actions::default();
        st.trigger_column(0, &mut act);
        assert_eq!(triggered(&act), vec![0, 1, 2]);
        assert!(st.scene_run.is_some());
        tick(&mut st, &[], 1.0);
        assert!(triggered(&tick(&mut st, &[0, 1], 5.0)).is_empty(), "layer 3 still plays");
        let act = tick(&mut st, &[0, 1, 2], 6.0);
        assert_eq!(triggered(&act), vec![0], "scene 2");
        assert_eq!(st.playing, vec![Some((0, 1)), None, None, None]);
        tick(&mut st, &[], 7.0);
        tick(&mut st, &[0], 9.0); // scene 3
        assert_eq!(st.playing[0], Some((0, 2)));
        tick(&mut st, &[], 10.0);
        let act = tick(&mut st, &[0], 12.0);
        assert!(triggered(&act).is_empty() && st.scene_run.is_none(), "stop after the last scene");
    }

    #[test]
    fn a_looping_scene_chain_goes_back_to_the_first_scene() {
        let mut st = show();
        st.project.decks[0].make_scene_chain(&[1, 2]);
        st.project.decks[0].scene_chains[0].looping = true;
        st.trigger_column(1, &mut Actions::default());
        tick(&mut st, &[], 1.0);
        tick(&mut st, &[0], 2.0);
        tick(&mut st, &[], 3.0);
        tick(&mut st, &[0], 4.0);
        assert_eq!(st.playing[0], Some((0, 1)), "scene 2 again");
        assert!(st.scene_run.is_some());
    }

    #[test]
    fn an_ab_loop_holds_the_scene_until_next() {
        let mut st = show();
        st.project.decks[0].make_scene_chain(&[1, 2]);
        let c = st.project.decks[0].clip_mut(0, 1).unwrap();
        (c.in_point, c.out_point) = (0.1, 0.5);
        let mut act = Actions::default();
        st.trigger_column(1, &mut act);
        let looping = act.commands.iter().any(|c| matches!(c, Command::Trigger { clip, .. } if clip.mode == PlayMode::Loop));
        assert!(looping, "an A–B loop keeps looping");
        assert!(triggered(&tick(&mut st, &[], 60.0)).is_empty());
        let mut act = Actions::default();
        next_now(&mut st, &mut act);
        assert_eq!(triggered(&act), vec![0], "Next moves on");
        assert_eq!(st.playing[0], Some((0, 2)));
    }

    #[test]
    fn a_scene_with_a_layer_chain_waits_for_the_chain() {
        let mut st = show();
        st.project.decks[0].make_scene_chain(&[0, 1]);
        st.project.decks[0].make_layer_chain(0, &[0, 1, 2]);
        let mut act = Actions::default();
        st.trigger_column(0, &mut act);
        assert_eq!(triggered(&act), vec![0], "only the chain's first layer starts");
        tick(&mut st, &[], 1.0);
        assert_eq!(triggered(&tick(&mut st, &[0], 2.0)), vec![1]);
        tick(&mut st, &[], 3.0);
        assert_eq!(triggered(&tick(&mut st, &[1], 4.0)), vec![2]);
        tick(&mut st, &[], 5.0);
        tick(&mut st, &[2], 6.0);
        assert_eq!(st.playing[0], Some((0, 1)), "the chain ended: scene 2");
    }

    #[test]
    fn other_triggers_stop_the_runs() {
        let mut st = show();
        st.project.decks[0].make_layer_chain(0, &[0, 1]);
        let mut act = Actions::default();
        st.trigger(0, 0, &mut act);
        st.trigger(0, 1, &mut act);
        assert!(st.layer_runs.is_empty());
        st.project.decks[0].make_scene_chain(&[1, 2]);
        st.trigger_column(1, &mut act);
        st.trigger(3, 5, &mut act); // an empty slot of a layer outside the scene
        assert!(st.scene_run.is_some(), "a layer outside the scene does not stop it");
        st.clear(0, &mut act);
        assert!(st.scene_run.is_none());
    }

    #[test]
    fn a_clip_that_fails_to_open_stops_the_chain() {
        let mut st = show();
        st.project.decks[0].make_layer_chain(0, &[0, 1]);
        st.trigger(0, 0, &mut Actions::default());
        tick(&mut st, &[], 1.0);
        tick(&mut st, &[0], 2.0);
        let mut s = snap(&st, &[]);
        s.layers[1] = LayerState { error: Some("gone".into()), ..LayerState::default() };
        update(&mut st, &s, 2.2, &mut Actions::default());
        assert_eq!(st.layer_runs.len(), 1, "it may still be opening");
        update(&mut st, &s, 3.0, &mut Actions::default());
        assert!(st.layer_runs.is_empty());
        assert!(st.status.contains("could not open"), "{}", st.status);
    }

    #[test]
    fn shift_clicked_slots_become_a_layer_chain_or_a_scene_chain() {
        let mut st = show();
        toggle_multi(&mut st, 0, 0);
        assert_eq!(pick(&st), None, "one slot is not a chain");
        toggle_multi(&mut st, 2, 0);
        assert_eq!(pick(&st), Some(Pick::Layers(0, vec![0, 2])), "one scene: its layers");
        toggle_multi(&mut st, 0, 1);
        toggle_multi(&mut st, 0, 2);
        assert_eq!(st.multi.len(), 4, "slots of other scenes add to the selection");
        assert_eq!(pick(&st), Some(Pick::Scenes(vec![0, 1, 2])), "several scenes: a scene chain");
        toggle_multi(&mut st, 0, 2);
        assert_eq!(st.multi.len(), 3, "a second click takes it out again");
        assert!(apply_pick(&mut st));
        assert_eq!(st.project.decks[0].scene_chains[0].cols, vec![0, 1]);
        assert!(st.multi.is_empty());
        toggle_scene(&mut st, 4);
        toggle_scene(&mut st, 1);
        assert_eq!(st.multi_scenes, vec![1, 4]);
        toggle_scene(&mut st, 4);
        assert_eq!(st.multi_scenes, vec![1]);
    }

    #[test]
    fn editing_a_running_chain_keeps_it_running() {
        let mut st = show();
        st.project.decks[0].make_layer_chain(0, &[0, 1]);
        st.project.decks[0].make_scene_chain(&[1, 2]);
        st.trigger(0, 0, &mut Actions::default());
        let mut ch = st.project.decks[0].layer_chains[0].clone();
        ch.looping = true;
        edit_layer_chain(&mut st, 0, 0, ch);
        tick(&mut st, &[], 1.0);
        assert!(st.layer_runs.len() == 1 && st.layer_runs[0].chain.looping, "the run follows the edit");
        st.trigger_column(1, &mut Actions::default());
        let mut sc = st.project.decks[0].scene_chains[0].clone();
        sc.looping = true;
        edit_scene_chain(&mut st, 0, 0, sc);
        tick(&mut st, &[], 2.0);
        assert!(st.scene_run.as_ref().is_some_and(|r| r.chain.looping));
        assert!(st.dirty);
    }
}
