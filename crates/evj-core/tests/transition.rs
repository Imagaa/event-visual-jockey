use evj_core::model::{Clip, Layer, Project};
use evj_core::transition::{Easing, TransitionPreset, TransitionRun, TransitionTime, resolve_preset};

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn time_in_seconds_or_beats() {
    assert!(close(TransitionTime::Seconds(1.5).seconds(120.0), 1.5));
    assert!(close(TransitionTime::Beats(2.0).seconds(120.0), 1.0));
    assert!(close(TransitionTime::Beats(1.0).seconds(60.0), 1.0));
}

#[test]
fn easing_curves() {
    for e in [Easing::Linear, Easing::In, Easing::Out, Easing::InOut] {
        assert!(close(e.apply(0.0), 0.0) && close(e.apply(1.0), 1.0), "{e:?}");
    }
    assert!(close(Easing::Linear.apply(0.25), 0.25));
    assert!(Easing::In.apply(0.25) < 0.25 && Easing::Out.apply(0.25) > 0.25);
    assert!(close(Easing::InOut.apply(0.5), 0.5));
}

#[test]
fn run_progress_and_finish() {
    let p = TransitionPreset { time: TransitionTime::Seconds(2.0), easing: Easing::Linear, ..TransitionPreset::crossfade() };
    let mut r = TransitionRun::new(&p, 120.0);
    assert!(close(r.progress(), 0.0));
    r.advance(0.5);
    assert!(close(r.progress(), 0.25) && !r.done());
    r.advance(10.0);
    assert!(close(r.progress(), 1.0) && r.done());
    let cut = TransitionRun::new(&TransitionPreset::cut(), 120.0);
    assert!(cut.done(), "cut finishes immediately");
}

#[test]
fn preset_resolution_order() {
    let mut p = Project::new_default();
    assert!(p.transitions.iter().any(|t| t.name == "Crossfade") && p.transitions.iter().any(|t| t.name == "Dip to Black"));
    let layer = Layer { transition: Some("Crossfade".into()), ..Layer::default() };
    let clip = Clip { transition: Some("Dip to Black".into()), ..Clip::default() };
    let plain = Clip::default();
    assert_eq!(resolve_preset(&p.transitions, &clip, None, &layer).unwrap().name, "Dip to Black", "clip override wins");
    assert_eq!(resolve_preset(&p.transitions, &plain, Some("Wipe"), &layer).unwrap().name, "Wipe", "next bar beats layer default");
    assert_eq!(resolve_preset(&p.transitions, &plain, None, &layer).unwrap().name, "Crossfade");
    assert!(resolve_preset(&p.transitions, &plain, None, &Layer::default()).is_none(), "none = cut");
    p.transitions.retain(|t| t.name != "Crossfade");
    assert!(resolve_preset(&p.transitions, &plain, None, &layer).is_none(), "deleted preset falls back to cut");
}
