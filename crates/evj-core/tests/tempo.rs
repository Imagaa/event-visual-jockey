use evj_core::lfo::{Lfo, Wave};
use evj_core::tempo::Tempo;

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

#[test]
fn beats_advance_with_bpm() {
    let mut t = Tempo::new(120.0);
    t.advance(1.0);
    assert!(close(t.beat, 2.0));
    t.set_bpm(60.0);
    t.advance(0.5);
    assert!(close(t.beat, 2.5));
    assert!(close(t.phase(), 0.5));
}

#[test]
fn tap_tempo_averages_recent_taps() {
    let mut t = Tempo::new(120.0);
    for i in 0..5 {
        t.tap(10.0 + i as f64 * 0.4); // 150 BPM
    }
    assert!((t.bpm - 150.0).abs() < 0.01, "{}", t.bpm);
    // Tapping aligns the beat phase to the tap.
    assert!(close(t.phase(), 0.0));
}

#[test]
fn tap_after_a_pause_starts_over() {
    let mut t = Tempo::new(128.0);
    t.tap(1.0);
    t.tap(1.5); // 120
    t.tap(10.0); // > 2 s later: new sequence, one tap does not change BPM
    assert!((t.bpm - 120.0).abs() < 0.01, "{}", t.bpm);
    t.tap(10.25); // 240
    assert!((t.bpm - 240.0).abs() < 0.01, "{}", t.bpm);
}

#[test]
fn bpm_is_clamped_and_resync_nudge() {
    let mut t = Tempo::new(120.0);
    t.set_bpm(5000.0);
    assert_eq!(t.bpm, 999.0);
    t.set_bpm(120.0);
    t.advance(0.3);
    t.resync();
    assert!(close(t.phase(), 0.0));
    t.nudge(0.1);
    assert!(close(t.phase(), 0.1));
    t.nudge(-0.2);
    assert!(close(t.phase(), 0.9));
}

#[test]
fn lfo_waves() {
    let l = |wave| Lfo { wave, beats: 4.0, min: 0.0, max: 1.0 };
    assert!(close(l(Wave::Sine).value(1.0), 1.0), "sine peaks at a quarter period");
    assert!(close(l(Wave::Sine).value(3.0), 0.0));
    assert!(close(l(Wave::Saw).value(2.0), 0.5));
    assert!(close(l(Wave::Square).value(1.0), 1.0));
    assert!(close(l(Wave::Square).value(3.0), 0.0));
    assert!(close(l(Wave::Triangle).value(2.0), 1.0));
    let r = l(Wave::Random);
    assert_eq!(r.value(0.1), r.value(3.9), "sample & hold within a period");
    assert!((0.0..=1.0).contains(&r.value(0.1)));
    assert_ne!(r.value(0.1), r.value(4.1), "new value next period");
}

#[test]
fn lfo_maps_into_min_max() {
    let l = Lfo { wave: Wave::Saw, beats: 1.0, min: 0.2, max: 0.6 };
    assert!(close(l.value(0.5), 0.4));
    let zero = Lfo { wave: Wave::Sine, beats: 0.0, min: 0.0, max: 1.0 };
    assert!(zero.value(1.0).is_finite());
}
