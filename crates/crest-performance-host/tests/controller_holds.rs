//! Protocol-only hold ownership tests; sensor/firmware tests live in the simulator.
use crest_linnstrument::Geometry;
use crest_performance_core::*;
use crest_performance_host::TestHost;

fn host() -> TestHost {
    let mut session = Session::default();
    let pattern = PatternSettings {
        steps: 2,
        pulses: 2,
        step: BeatTime(TICKS_PER_BEAT),
        gate: BeatTime(TICKS_PER_BEAT / 2),
        tone_min: 0,
        tone_max: 0,
        ..PatternSettings::default()
    };
    session.slots[0].clip = Some(
        pattern
            .render(HarmonySettings::default(), TrackId(1))
            .unwrap(),
    );
    let mut host = TestHost::new(session, Geometry::Pads200).unwrap();
    host.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
    host
}
fn pad(host: &mut TestHost, row: u8, column: u8, pressed: bool) {
    for byte in [(if pressed { 0x90 } else { 0x80 }) | row, column, 90] {
        host.accept_bytes(0, byte, Geometry::Pads200).unwrap();
    }
}
fn mute(host: &mut TestHost, muted: bool) {
    host.command(Command::SetClipMuted { slot: 0, muted })
        .unwrap();
}

#[test]
fn fill_combines_independent_surfaces_and_late_releases_after_stop_are_harmless() {
    let mut h = host();
    let saved = h.document().unwrap();
    pad(&mut h, 6, 1, true);
    h.command(Command::SetFill(true)).unwrap();
    pad(&mut h, 6, 1, false);
    assert!(h.engine.snapshot().fill, "host still owns Fill");
    pad(&mut h, 6, 1, true);
    h.command(Command::SetFill(false)).unwrap();
    assert!(h.engine.snapshot().fill, "pad still owns Fill");
    h.command(Command::Stop).unwrap();
    h.command(Command::SetFill(true)).unwrap();
    pad(&mut h, 6, 1, false);
    assert!(
        h.engine.snapshot().fill,
        "old pad release cannot clear new host hold"
    );
    h.command(Command::Stop).unwrap();
    pad(&mut h, 6, 1, true);
    h.command(Command::SetFill(false)).unwrap();
    assert!(
        h.engine.snapshot().fill,
        "old host release cannot clear new pad hold"
    );
    pad(&mut h, 6, 1, false);
    assert!(!h.engine.snapshot().fill);
    assert_eq!(h.document().unwrap(), saved);
    assert_eq!(h.output_count(), 0, "Fill controls emit no notes");
}

#[test]
fn mute_is_owned_per_surface_and_per_player_lifetime() {
    let mut h = host();
    pad(&mut h, 1, 1, true); // stopped pad is dark and inert
    pad(&mut h, 0, 1, true);
    assert!(!h.engine.snapshot().muted_slots[0]);
    pad(&mut h, 1, 1, true);
    mute(&mut h, true);
    let count = h.output_count();
    pad(&mut h, 1, 1, true); // duplicate attack cannot multiply holds
    pad(&mut h, 1, 1, false);
    assert!(h.engine.snapshot().muted_slots[0]);
    assert_eq!(h.output_count(), count, "no duplicate releases");
    pad(&mut h, 1, 1, true);
    mute(&mut h, false);
    assert!(h.engine.snapshot().muted_slots[0]);
    pad(&mut h, 0, 1, true); // stopping retires old holds
    pad(&mut h, 0, 1, true); // new launch starts unmuted
    assert!(!h.engine.snapshot().muted_slots[0]);
    mute(&mut h, true);
    pad(&mut h, 1, 1, false); // release from previous player
    assert!(h.engine.snapshot().muted_slots[0]);
    mute(&mut h, false);
    assert!(!h.engine.snapshot().muted_slots[0]);
    let saved = h.document().unwrap();
    pad(&mut h, 1, 1, true);
    assert_eq!(h.document().unwrap(), saved);
    h.disconnect().unwrap();
    assert_eq!(h.engine.snapshot().active_voices, 0);
    h.launch(0).unwrap();
    pad(&mut h, 1, 1, true);
    mute(&mut h, false);
    assert!(h.engine.snapshot().muted_slots[0]);
    h.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
    assert!(!h.engine.snapshot().muted_slots[0]);
    mute(&mut h, true);
    pad(&mut h, 1, 1, false);
    assert!(h.engine.snapshot().muted_slots[0]);
    h.command(Command::Stop).unwrap();
}

#[test]
fn rejected_mute_does_not_claim_a_hold_and_can_be_retried_after_draining() {
    let mut h = host();
    h.engine = Engine::new(
        API_VERSION,
        h.engine.save_session().unwrap(),
        Limits {
            voices: 1,
            output_events: 1,
            recorded_events: 4,
        },
    )
    .unwrap();
    h.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
    h.launch(0).unwrap();
    h.engine.advance(BeatTime(0)).unwrap(); // leave one attack undrained
    assert!(matches!(
        h.command(Command::SetClipMuted {
            slot: 0,
            muted: true
        }),
        Err(Error::Capacity(_))
    ));
    h.advance_micros(0).unwrap();
    pad(&mut h, 1, 1, false);
    assert!(
        !h.engine.snapshot().muted_slots[0],
        "rejected host hold was not stored"
    );
    assert_eq!(h.engine.snapshot().active_voices, 1);
    mute(&mut h, true);
    assert!(h.engine.snapshot().muted_slots[0]);
    assert_eq!(h.engine.snapshot().active_voices, 0);
    pad(&mut h, 1, 1, true);
    mute(&mut h, false);
    assert!(h.engine.snapshot().muted_slots[0]);
    pad(&mut h, 1, 1, false);
    assert!(!h.engine.snapshot().muted_slots[0]);
}
