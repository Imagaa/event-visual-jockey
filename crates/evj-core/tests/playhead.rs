use evj_core::model::{Clip, PlayMode};
use evj_core::playhead::{Playhead, effective_speed, frame_index};
use std::path::PathBuf;

fn clip(mode: PlayMode, speed: f64) -> Clip {
    Clip { mode, speed, ..Clip::new(PathBuf::from("x.mov")) }
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn loop_wraps_inside_in_out() {
    let c = Clip { in_point: 0.25, out_point: 0.75, ..clip(PlayMode::Loop, 1.0) }; // 10 s clip → 2.5..7.5
    let mut p = Playhead::start(&c, 10.0);
    assert!(close(p.pos, 2.5));
    p.advance(6.0, &c, 10.0, 120.0); // 2.5 + 6 = 8.5 → 3.5
    assert!(close(p.pos, 3.5), "{}", p.pos);
    assert!(!p.finished);
}

#[test]
fn ping_pong_reflects_and_flips_direction() {
    let c = clip(PlayMode::PingPong, 1.0);
    let mut p = Playhead::start(&c, 4.0);
    p.advance(5.0, &c, 4.0, 120.0); // 0 → 4 → back to 3
    assert!(close(p.pos, 3.0), "{}", p.pos);
    assert_eq!(p.dir, -1.0);
    p.advance(4.0, &c, 4.0, 120.0); // 3 → 0 → 1
    assert!(close(p.pos, 1.0), "{}", p.pos);
    assert_eq!(p.dir, 1.0);
}

#[test]
fn once_clamps_and_finishes() {
    let c = clip(PlayMode::Once, 1.0);
    let mut p = Playhead::start(&c, 3.0);
    p.advance(2.0, &c, 3.0, 120.0);
    assert!(!p.finished);
    p.advance(2.0, &c, 3.0, 120.0);
    assert!(p.finished);
    assert!(close(p.pos, 3.0));
}

#[test]
fn negative_speed_runs_backwards_and_wraps() {
    let c = clip(PlayMode::Loop, -2.0);
    let mut p = Playhead::start(&c, 10.0);
    assert!(close(p.pos, 10.0));
    p.advance(1.0, &c, 10.0, 120.0);
    assert!(close(p.pos, 8.0), "{}", p.pos);
    p.advance(5.0, &c, 10.0, 120.0); // 8 - 10 = -2 → 8
    assert!(close(p.pos, 8.0), "{}", p.pos);
}

#[test]
fn once_backwards_finishes_at_in_point() {
    let c = clip(PlayMode::Once, -1.0);
    let mut p = Playhead::start(&c, 2.0);
    p.advance(3.0, &c, 2.0, 120.0);
    assert!(p.finished && close(p.pos, 0.0));
}

#[test]
fn bpm_sync_speed() {
    let c = Clip { bpm_beats: Some(4.0), ..clip(PlayMode::Loop, 1.0) };
    // 4 beats at 120 BPM = 2 s; a 4 s clip must play at 2x.
    assert!(close(effective_speed(&c, 4.0, 120.0), 2.0));
    let rev = Clip { speed: -1.0, ..c };
    assert!(close(effective_speed(&rev, 4.0, 120.0), -2.0));
}

#[test]
fn zero_length_range_is_safe() {
    let c = Clip { in_point: 0.5, out_point: 0.5, ..clip(PlayMode::Loop, 1.0) };
    let mut p = Playhead::start(&c, 10.0);
    p.advance(1.0, &c, 10.0, 120.0);
    assert!(close(p.pos, 5.0));
    let z = clip(PlayMode::PingPong, 1.0);
    let mut q = Playhead::start(&z, 0.0);
    q.advance(1.0, &z, 0.0, 120.0);
    assert!(q.pos.is_finite());
}

#[test]
fn frame_index_clamps() {
    assert_eq!(frame_index(0.0, 30.0, 90), 0);
    assert_eq!(frame_index(1.0, 30.0, 90), 30);
    assert_eq!(frame_index(2.9999, 30.0, 90), 89);
    assert_eq!(frame_index(3.0, 30.0, 90), 89);
    assert_eq!(frame_index(-1.0, 30.0, 90), 0);
}

#[test]
fn remaining_counts_to_the_end_in_the_play_direction() {
    use evj_core::playhead::remaining;
    assert_eq!(remaining(2.0, 0.0, 10.0, 1.0), 8.0);
    assert_eq!(remaining(2.0, 0.0, 10.0, 2.0), 4.0, "double speed");
    assert_eq!(remaining(2.0, 1.0, 10.0, -1.0), 1.0, "reverse runs to the in point");
    assert_eq!(remaining(12.0, 0.0, 10.0, 1.0), 0.0, "never negative");
    assert!(remaining(2.0, 0.0, 10.0, 0.0).is_infinite(), "paused / speed 0");
}

#[test]
fn in_out_is_ordered_seconds() {
    use evj_core::model::Clip;
    use evj_core::playhead::in_out;
    let c = Clip { in_point: 0.8, out_point: 0.2, ..Clip::new("a.mov".into()) };
    assert_eq!(in_out(&c, 10.0), (2.0, 8.0));
}
