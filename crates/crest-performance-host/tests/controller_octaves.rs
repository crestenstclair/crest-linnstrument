//! Protocol tests; the simulator separately exercises firmware sensor contacts.
use crest_linnstrument::Geometry;
use crest_performance_core::*;
use crest_performance_host::{OctaveDirection, TestHost};

fn host() -> TestHost {
    let mut host = TestHost::new(Session::default(), Geometry::Pads200).unwrap();
    host.command(Command::Generate {
        slot: 0,
        settings: PatternSettings {
            tone_min: 0,
            tone_max: 0,
            ..PatternSettings::default()
        },
    })
    .unwrap();
    host.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
    host.launch(0).unwrap();
    host
}
fn pad(host: &mut TestHost, row: u8, pressed: bool) {
    for byte in [(if pressed { 0x90 } else { 0x80 }) | row, 1, 90] {
        host.accept_bytes(0, byte, Geometry::Pads200).unwrap();
    }
}
fn shift(host: &TestHost) -> i8 {
    host.engine.snapshot().clip_transpositions[0]
}

#[test]
fn independent_directions_and_sources_compose_without_losing_a_held_owner() {
    let mut h = host();
    let saved = h.document().unwrap();
    h.hold_clip_octave(0, OctaveDirection::Up, true).unwrap();
    pad(&mut h, 4, true);
    h.hold_clip_octave(0, OctaveDirection::Up, false).unwrap();
    assert_eq!(shift(&h), 12);
    pad(&mut h, 3, true);
    assert_eq!(shift(&h), 0, "opposite physical fingers cancel");
    h.command(Command::SetFill(true)).unwrap(); // unrelated reconciliation keeps both owners
    pad(&mut h, 4, false);
    assert_eq!(shift(&h), -12);
    h.hold_clip_octave(0, OctaveDirection::Down, true).unwrap();
    pad(&mut h, 3, false);
    assert_eq!(shift(&h), -12);
    h.hold_clip_octave(0, OctaveDirection::Up, true).unwrap();
    assert_eq!(shift(&h), 0);
    h.hold_clip_octave(0, OctaveDirection::Down, false).unwrap();
    assert_eq!(shift(&h), 12);
    h.hold_clip_octave(0, OctaveDirection::Up, false).unwrap();
    assert_eq!(shift(&h), 0);
    assert_eq!(h.document().unwrap(), saved);
}

#[test]
fn zero_sum_holds_retire_on_relaunch_mode_stop_and_disconnect() {
    let mut h = host();
    for reset in [
        Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        },
        Command::SetMode(SurfaceMode::Clips),
        Command::Stop,
    ] {
        h.launch(0).unwrap();
        pad(&mut h, 4, true);
        pad(&mut h, 3, true);
        assert_eq!(shift(&h), 0);
        h.command(reset).unwrap();
        h.launch(0).unwrap();
        h.hold_clip_octave(0, OctaveDirection::Down, true).unwrap();
        pad(&mut h, 3, false);
        pad(&mut h, 4, false);
        assert_eq!(
            shift(&h),
            -12,
            "old contacts cannot cancel new host ownership"
        );
        h.hold_clip_octave(0, OctaveDirection::Down, false).unwrap();
    }
    pad(&mut h, 3, true);
    pad(&mut h, 4, true);
    h.disconnect().unwrap();
    h.launch(0).unwrap();
    h.hold_clip_octave(0, OctaveDirection::Up, true).unwrap();
    pad(&mut h, 3, false);
    assert_eq!(shift(&h), 12);
}

#[test]
fn failed_press_never_acquires_ownership_and_stopped_releases_are_safe() {
    let mut h = host();
    h.command(Command::Stop).unwrap();
    assert!(h.hold_clip_octave(0, OctaveDirection::Up, true).is_err());
    h.hold_clip_octave(0, OctaveDirection::Up, false).unwrap();
    h.command(Command::SetHarmony(HarmonySettings {
        tonic: 120,
        ..HarmonySettings::default()
    }))
    .unwrap();
    h.command(Command::Generate {
        slot: 0,
        settings: PatternSettings {
            tone_min: 0,
            tone_max: 0,
            ..PatternSettings::default()
        },
    })
    .unwrap();
    h.launch(0).unwrap();
    assert!(h.hold_clip_octave(0, OctaveDirection::Up, true).is_err());
    h.hold_clip_octave(0, OctaveDirection::Down, true).unwrap();
    assert_eq!(
        shift(&h),
        -12,
        "rejected up hold never becomes active later"
    );
    h.hold_clip_octave(0, OctaveDirection::Up, true).unwrap();
    assert_eq!(shift(&h), 0);
    assert!(h.hold_clip_octave(0, OctaveDirection::Down, false).is_err());
    assert_eq!(
        shift(&h),
        0,
        "invalid remaining direction clears to original phrase"
    );
    h.hold_clip_octave(0, OctaveDirection::Up, false).unwrap();
    assert_eq!(shift(&h), 0, "released owner must never reappear");
}
