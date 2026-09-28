//! Clip sequences: slots of one layer that play one after another. The app plans the order;
//! the engine plays the queued next clip the moment the current one finishes.
use crate::ui::{Actions, UiState};
use evj_core::model::{Clip, PlayMode, Sequence};
use evj_engine::{Command, Snapshot};

/// A sequence playing on a layer.
#[derive(Clone, Debug, PartialEq)]
pub struct SeqRun {
    pub deck: usize,
    pub layer: usize,
    /// The sequence's slots when the run started: an edited sequence ends the run (indexes
    /// into `Deck::sequences` shift when one is broken or pruned).
    pub cols: Vec<usize>,
    /// Position in `cols` of the clip on air.
    pub index: usize,
    /// The layer's `queue_taken` / `queue_failed` already accounted for.
    pub seen: u64,
    pub failed: u64,
}

fn is_still(clip: &Clip) -> bool {
    evj_media::image::is_image(&clip.path)
}

/// How a slot plays inside a sequence: once, stills timed. An A–B loop (Loop with a start / end
/// set) keeps looping until Next — plain clips default to Loop, so that alone does not count.
pub fn play_clip(seq: &Sequence, clip: &Clip) -> Clip {
    let mut c = clip.clone();
    let ab_loop = c.mode == PlayMode::Loop && (c.in_point > 0.0 || c.out_point < 1.0);
    if !ab_loop {
        c.mode = PlayMode::Once;
    }
    // Also with attached audio: its length wins when it plays; a missing song must not hold
    // the sequence forever.
    if is_still(&c) {
        c.still_secs = Some(seq.still_secs);
    }
    c
}

pub fn next_index(seq: &Sequence, index: usize) -> Option<usize> {
    if index + 1 < seq.cols.len() {
        Some(index + 1)
    } else if seq.loop_all {
        Some(0)
    } else {
        None
    }
}

/// The run's sequence, as long as it has not been edited.
fn sequence<'a>(st: &'a UiState, run: &SeqRun) -> Option<&'a Sequence> {
    st.project.decks.get(run.deck)?.sequences.iter().find(|s| s.layer == run.layer && s.cols == run.cols)
}

/// Column and clip (as played in the sequence) at `index`.
fn clip_at(st: &UiState, run: &SeqRun, index: usize) -> Option<(usize, Clip)> {
    let seq = sequence(st, run)?;
    let col = *seq.cols.get(index)?;
    let clip = st.project.decks.get(run.deck)?.clip(seq.layer, col)?;
    Some((col, play_clip(seq, clip)))
}

/// A settings change of the slot on air: inside a running sequence it keeps playing as the
/// sequence plays it (once, timed still), or the sequence would stall.
pub fn as_played(st: &UiState, layer: usize, col: usize, clip: Clip) -> Clip {
    let Some(run) = st.runs.get(layer).cloned().flatten() else { return clip };
    match sequence(st, &run) {
        Some(seq) if run.cols.get(run.index) == Some(&col) && run.deck == st.project.active_deck => play_clip(seq, &clip),
        _ => clip,
    }
}

/// Tells the engine what follows the clip on air (nothing after the last one).
pub fn queue(st: &UiState, layer: usize, act: &mut Actions) {
    let Some(run) = st.runs.get(layer).cloned().flatten() else { return };
    let next = sequence(st, &run).and_then(|seq| next_index(seq, run.index)).and_then(|i| clip_at(st, &run, i)).map(|(_, c)| c);
    act.commands.push(Command::QueueNext { layer, clip: next });
}

/// A slot goes to Program: when it belongs to a sequence, a run starts there and the clip to
/// play (once / timed still) is returned; otherwise any run on the layer ends.
pub fn start(st: &mut UiState, layer: usize, col: usize) -> Option<Clip> {
    if st.runs.len() <= layer {
        st.runs.resize(layer + 1, None);
    }
    let deck = st.project.active_deck;
    let found = st.project.deck().and_then(|d| d.sequence_at(layer, col)).map(|(_, s)| (s.cols.clone(), s.cols.iter().position(|&c| c == col).unwrap_or(0)));
    let Some((cols, index)) = found else {
        st.runs[layer] = None;
        return None;
    };
    let seen = st.last_taken.get(layer).copied().unwrap_or(0);
    let failed = st.last_failed.get(layer).copied().unwrap_or(0);
    let run = SeqRun { deck, layer, cols, index, seen, failed };
    let clip = clip_at(st, &run, index).map(|(_, c)| c);
    st.runs[layer] = Some(run);
    clip
}

/// Every frame: a queued clip went on air → the run moves on and queues the one after.
pub fn update(st: &mut UiState, snap: &Snapshot, act: &mut Actions) {
    st.last_taken = snap.layers.iter().map(|l| l.queue_taken).collect();
    st.last_failed = snap.layers.iter().map(|l| l.queue_failed).collect();
    for layer in 0..st.runs.len() {
        let Some(mut run) = st.runs[layer].clone() else { continue };
        let Some(state) = snap.layers.get(layer) else { continue };
        // The next clip could not open: the current one stays on, the run ends.
        if state.queue_failed > run.failed {
            st.runs[layer] = None;
            let why = state.error.clone().unwrap_or_default();
            st.status = format!("Sequence on layer {} stopped: the next clip could not open. {why}", layer + 1);
            continue;
        }
        // The sequence was edited (broken, pruned, remade): end the run and its queued clip.
        if sequence(st, &run).is_none() {
            st.runs[layer] = None;
            act.commands.push(Command::QueueNext { layer, clip: None });
            continue;
        }
        let taken = state.queue_taken;
        if taken <= run.seen {
            continue;
        }
        run.seen = taken;
        let next = sequence(st, &run).and_then(|seq| next_index(seq, run.index));
        let Some((i, (col, _))) = next.and_then(|i| Some((i, clip_at(st, &run, i)?))) else {
            st.runs[layer] = None; // the last clip is on air
            continue;
        };
        run.index = i;
        if let Some(p) = st.playing.get_mut(layer) {
            *p = Some((run.deck, col));
        }
        if let Some(c) = st.project.decks.get_mut(run.deck).and_then(|d| d.clip_mut(layer, col)) {
            c.aired = true;
        }
        st.runs[layer] = Some(run);
        queue(st, layer, act);
    }
}

/// Next ⏭: the following clip of the layer's sequence now (with the layer transition).
pub fn next_now(st: &mut UiState, layer: usize, act: &mut Actions) {
    let Some(run) = st.runs.get(layer).cloned().flatten() else { return };
    let next = sequence(st, &run).and_then(|seq| next_index(seq, run.index)).and_then(|i| clip_at(st, &run, i));
    match next {
        Some((col, _)) if run.deck == st.project.active_deck => st.trigger(layer, col, act),
        Some(_) => st.status = "The sequence is on another deck".into(),
        None => st.status = "End of the sequence".into(),
    }
}

/// Shift+click: add / remove a slot; a slot of another layer starts a new selection.
pub fn toggle_multi(st: &mut UiState, layer: usize, col: usize) {
    if st.multi.first().is_some_and(|(l, _)| *l != layer) {
        st.multi.clear();
    }
    match st.multi.iter().position(|s| *s == (layer, col)) {
        Some(i) => {
            st.multi.remove(i);
        }
        None => {
            st.multi.push((layer, col));
            st.multi.sort_unstable();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use evj_core::model::{Clip, PlayMode, Project};
    use evj_engine::{Command, LayerState, Snapshot};
    use std::path::PathBuf;

    fn show() -> UiState {
        let mut p = Project::new_default();
        for c in 0..3 {
            p.decks[0].slots[0][c] = Some(Clip::new(format!("C:/m/{c}.mov").into()));
        }
        p.decks[0].slots[0][1].as_mut().unwrap().path = "C:/m/still.png".into();
        p.decks[0].make_sequence(0, &[0, 1, 2]);
        UiState::new(p)
    }

    fn snap(taken: u64) -> Snapshot {
        Snapshot { layers: vec![LayerState { queue_taken: taken, ..Default::default() }; 4], ..Default::default() }
    }

    fn queued(act: &Actions) -> Option<Clip> {
        act.commands.iter().rev().find_map(|c| match c {
            Command::QueueNext { clip, .. } => Some(clip.clone()),
            _ => None,
        })?
    }

    fn triggered(act: &Actions) -> Option<Clip> {
        act.commands.iter().find_map(|c| match c {
            Command::Trigger { clip, .. } => Some(clip.clone()),
            _ => None,
        })
    }

    #[test]
    fn triggering_a_sequence_slot_plays_once_and_queues_the_next() {
        let mut st = show();
        let mut act = Actions::default();
        st.trigger(0, 0, &mut act);
        assert_eq!(triggered(&act).unwrap().mode, PlayMode::Once);
        let next = queued(&act).unwrap();
        assert_eq!(next.path, PathBuf::from("C:/m/still.png"));
        assert_eq!(next.still_secs, Some(5.0));
        assert!(st.project.decks[0].clip(0, 0).unwrap().aired, "marked as aired");
    }

    #[test]
    fn a_looping_clip_keeps_looping_inside_a_sequence() {
        let mut st = show();
        let c = st.project.decks[0].slots[0][0].as_mut().unwrap();
        (c.mode, c.in_point, c.out_point) = (PlayMode::Loop, 0.2, 0.6); // an A–B loop
        let mut act = Actions::default();
        st.trigger(0, 0, &mut act);
        assert_eq!(triggered(&act).unwrap().mode, PlayMode::Loop, "until Next");
    }

    #[test]
    fn the_run_advances_and_ends_or_loops() {
        let mut st = show();
        let mut act = Actions::default();
        st.trigger(0, 0, &mut act);
        let mut act = Actions::default();
        update(&mut st, &snap(1), &mut act);
        assert_eq!(st.runs[0].as_ref().unwrap().index, 1);
        assert_eq!(queued(&act).unwrap().path, PathBuf::from("C:/m/2.mov"));
        assert_eq!(st.playing[0], Some((0, 1)), "the Program tally follows the run");
        let mut act = Actions::default();
        update(&mut st, &snap(2), &mut act);
        assert!(queued(&act).is_none(), "last clip: nothing queued");
        st.project.decks[0].sequences[0].loop_all = true;
        let mut act = Actions::default();
        st.trigger(0, 2, &mut act);
        assert_eq!(queued(&act).unwrap().path, PathBuf::from("C:/m/0.mov"), "loop: back to the first");
    }

    #[test]
    fn other_triggers_and_clear_stop_the_run() {
        let mut st = show();
        st.project.decks[0].slots[0][4] = Some(Clip::new("C:/m/solo.mov".into()));
        let mut act = Actions::default();
        st.trigger(0, 0, &mut act);
        let mut act = Actions::default();
        st.trigger(0, 4, &mut act);
        assert!(st.runs[0].is_none());
        assert!(act.commands.iter().any(|c| matches!(c, Command::QueueNext { clip: None, .. })), "the queue is dropped");
        st.trigger(0, 0, &mut act);
        st.clear(0, &mut act);
        assert!(st.runs[0].is_none());
    }

    #[test]
    fn next_now_triggers_the_following_clip() {
        let mut st = show();
        let mut act = Actions::default();
        st.trigger(0, 0, &mut act);
        let mut act = Actions::default();
        next_now(&mut st, 0, &mut act);
        assert_eq!(triggered(&act).map(|c| c.path), Some(PathBuf::from("C:/m/still.png")));
        assert_eq!(st.runs[0].as_ref().unwrap().index, 1);
    }

    fn snap_failed(failed: u64) -> Snapshot {
        Snapshot { layers: vec![LayerState { queue_failed: failed, ..Default::default() }; 4], ..Default::default() }
    }

    #[test]
    fn a_sequence_edited_while_running_never_plays_another_layers_clip() {
        let mut st = show();
        for c in 0..2 {
            st.project.decks[0].slots[2][c] = Some(Clip::new(format!("C:/m/top{c}.mov").into()));
        }
        st.project.decks[0].make_sequence(2, &[0, 1]);
        let mut act = Actions::default();
        st.trigger(0, 0, &mut act);
        st.project.decks[0].break_sequence(0); // the layer-2 sequence moves to index 0
        let mut act = Actions::default();
        update(&mut st, &snap(1), &mut act);
        let wrong = queued(&act).is_some_and(|c| c.path.to_string_lossy().contains("top"));
        assert!(!wrong, "a clip of another layer was queued");
        assert!(st.runs[0].is_none(), "the edited sequence's run ends");
    }

    #[test]
    fn editing_the_on_air_clip_keeps_it_playing_as_in_the_sequence() {
        let mut st = show();
        let mut act = Actions::default();
        st.trigger(0, 0, &mut act);
        // A settings edit (fit) of the slot on air: the timeline and the properties panel both
        // send it through `as_played`.
        let mut c = st.project.decks[0].clip(0, 0).unwrap().clone();
        c.fit = evj_core::model::FitMode::Fill;
        assert_eq!(as_played(&st, 0, 0, c.clone()).mode, PlayMode::Once, "still plays once inside the sequence");
        assert_eq!(as_played(&st, 0, 1, c.clone()).mode, PlayMode::Loop, "another slot is not the one on air");
        let mut still = st.project.decks[0].clip(0, 1).unwrap().clone();
        still.fit = evj_core::model::FitMode::Fill;
        let mut act = Actions::default();
        update(&mut st, &snap(1), &mut act);
        assert_eq!(as_played(&st, 0, 1, still).still_secs, Some(5.0), "the still keeps its time");
    }

    #[test]
    fn a_still_with_attached_audio_still_gets_a_duration() {
        let st = show();
        let mut c = st.project.decks[0].clip(0, 1).unwrap().clone();
        c.attached = Some(evj_core::model::AttachedAudio { path: "C:/gone/song.mp3".into(), ..Default::default() });
        assert_eq!(play_clip(&st.project.decks[0].sequences[0], &c).still_secs, Some(5.0), "a missing song must not hold the sequence");
    }

    #[test]
    fn a_queued_clip_that_fails_stops_the_run() {
        let mut st = show();
        let mut act = Actions::default();
        st.trigger(0, 0, &mut act);
        let mut act = Actions::default();
        update(&mut st, &snap_failed(1), &mut act);
        assert!(st.runs[0].is_none());
        assert_eq!(st.playing[0], Some((0, 0)), "the tally stays on the clip still on air");
        assert!(!st.project.decks[0].clip(0, 1).unwrap().aired);
        assert!(st.status.contains("stopped"), "{}", st.status);
    }

    #[test]
    fn shift_click_multi_select_stays_in_one_layer() {
        let mut st = show();
        toggle_multi(&mut st, 0, 0);
        toggle_multi(&mut st, 0, 2);
        assert_eq!(st.multi, vec![(0, 0), (0, 2)]);
        toggle_multi(&mut st, 0, 0);
        assert_eq!(st.multi, vec![(0, 2)], "a second click removes it");
        toggle_multi(&mut st, 1, 1);
        assert_eq!(st.multi, vec![(1, 1)], "another layer starts over");
    }
}
