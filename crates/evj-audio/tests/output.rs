use evj_audio::AudioEngine;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../evj-media/tests/fixtures").join(name)
}

fn wait(mut ok: impl FnMut() -> bool) -> bool {
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(3) {
        if ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn plays_through_the_default_device() {
    let Ok(mut a) = AudioEngine::start(None) else {
        eprintln!("no audio output device: skipped");
        return;
    };
    assert!(a.rate() >= 8000);
    assert!(!AudioEngine::devices().is_empty());
    // very quiet so the test does not blast the speakers
    a.master(0.02, 0.0);
    let h = a.play(1, &fixture("tone_44k_mono.mp3"), 1.0, 0.0, false).unwrap();
    assert!(wait(|| h.position_secs(a.rate()) > 0.2), "audio clock runs");
    a.stop(1, 0.05);
    assert!(wait(|| h.finished()), "stop fades out and ends the voice");
    a.poll();
}

#[test]
fn start_offset_and_bad_files() {
    let Ok(mut a) = AudioEngine::start(None) else { return };
    a.master(0.0, 0.0);
    let h = a.play(7, &fixture("tone_44k_mono.mp3"), 1.0, 0.8, false).unwrap();
    assert!(wait(|| h.finished()), "0.2 s left after starting at 0.8 s");
    // Opening happens off the caller's thread: errors arrive on the handle.
    for (id, bad) in [(8, PathBuf::from("C:/nope/x.mp3")), (9, fixture("h264_320x240.mp4"))] {
        let h = a.play(id, &bad, 1.0, 0.0, false).unwrap();
        assert!(wait(|| h.finished()), "{}", bad.display());
        assert!(h.error().is_some(), "{}", bad.display());
    }
}

#[test]
fn a_pair_beyond_the_device_is_an_error() {
    let Some((name, ch)) = AudioEngine::devices().into_iter().next() else {
        eprintln!("no audio output device: skipped");
        return;
    };
    let route = evj_audio::Route { device: Some(name), first_channel: ch + 2 };
    let e = AudioEngine::start_route(&route, false).err().expect("refused");
    assert!(format!("{e:#}").contains("channels"), "{e:#}");
}

#[test]
fn preview_route_does_not_fall_back() {
    let route = evj_audio::Route { device: Some("No Such Headphones 123".into()), first_channel: 0 };
    assert!(AudioEngine::start_route(&route, false).is_err(), "a missing device is an error, not the default device");
    if AudioEngine::default_device().is_some() {
        assert!(AudioEngine::start_route(&route, true).is_ok(), "Program still falls back");
    }
}

