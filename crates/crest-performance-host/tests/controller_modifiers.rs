//! Protocol fixtures; firmware/sensor evidence lives in the simulator suite.
use crest_linnstrument::Geometry;
use crest_performance_core::*;
use crest_performance_host::TestHost;

fn pad(host: &mut TestHost, column: u8, pressed: bool) {
    for byte in [if pressed { 0x95 } else { 0x85 }, column, 90] {
        host.accept_bytes(0, byte, Geometry::Pads128).unwrap();
    }
}
fn host(session: Session) -> TestHost {
    let mut host = TestHost::new(session, Geometry::Pads128).unwrap();
    host.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
    host
}

#[test]
fn extension_and_suspension_pads_select_exclusively_toggle_off_and_combine_with_borrowing() {
    let mut h = host(Session::default());
    for (column, extension, suspension, borrowed) in [
        (1, ChordExtension::Seventh, Suspension::None, false),
        (2, ChordExtension::Ninth, Suspension::None, false),
        (2, ChordExtension::Triad, Suspension::None, false),
        (3, ChordExtension::Triad, Suspension::Second, false),
        (4, ChordExtension::Triad, Suspension::Fourth, false),
        (4, ChordExtension::Triad, Suspension::None, false),
        (5, ChordExtension::Triad, Suspension::None, true),
        (1, ChordExtension::Seventh, Suspension::None, true),
    ] {
        pad(&mut h, column, true);
        pad(&mut h, column, true); // duplicate attack is not a second toggle
        pad(&mut h, column, false);
        let settings = h.engine.snapshot().harmony;
        assert_eq!(
            (settings.extension, settings.suspension, settings.borrowed),
            (extension, suspension, borrowed)
        );
    }
    assert_eq!(h.output_count(), 0);
    let reopened = TestHost::from_document(h.document().unwrap(), Geometry::Pads128).unwrap();
    assert_eq!(
        reopened.engine.snapshot().harmony,
        h.engine.snapshot().harmony
    );
    // Unassigned columns and the firmware control column never change harmony.
    let saved = h.document().unwrap();
    for column in [0, 12, 13, 14, 16] {
        pad(&mut h, column, true);
        pad(&mut h, column, false);
    }
    assert_eq!(h.document().unwrap(), saved);
}

#[test]
fn unavailable_modifiers_leave_learned_voicings_and_high_pitch_sources_unchanged() {
    let mut session = Session::default();
    session.harmony.learned[0] = Some(LearnedChord::new(&[60, 67, 72], 60).unwrap());
    session.harmony_source = HarmonySource::Clip { slot: 0 };
    let mut h = host(session);
    let saved = h.document().unwrap();
    for column in 1..=5 {
        pad(&mut h, column, true);
        pad(&mut h, column, false);
    }
    assert_eq!(h.document().unwrap(), saved);
    h.command(Command::SetHarmony(HarmonySettings {
        tonic: 120,
        ..HarmonySettings::default()
    }))
    .unwrap();
    let saved = h.document().unwrap();
    for column in [1, 2] {
        pad(&mut h, column, true);
        pad(&mut h, column, false);
    }
    assert_eq!(h.document().unwrap(), saved);
    assert_eq!(h.output_count(), 0);
}

#[test]
fn modifiers_preserve_sounding_expression_and_releases_in_running_clips() {
    let mut session = Session::default();
    let note = NoteKey {
        source: SourceId(1),
        track: TrackId(1),
        note: NoteId(1),
    };
    let expression = Expression {
        pitch: 900_000,
        pressure: 50_000,
        timbre: 20_000,
    };
    session.slots[0].clip = Some(Clip {
        length: BeatTime(100),
        events: vec![
            InputEvent {
                at: BeatTime(0),
                note,
                gesture: Gesture::On {
                    key: 60,
                    velocity: 90,
                    expression: Expression::default(),
                },
            },
            InputEvent {
                at: BeatTime(10),
                note,
                gesture: Gesture::Expression { expression },
            },
            InputEvent {
                at: BeatTime(90),
                note,
                gesture: Gesture::Off { velocity: 17 },
            },
        ],
        harmony_origin: vec![],
        pattern_origin: vec![],
        follow: None,
        conditions: vec![],
        parameter_locks: Vec::new(),
    });
    let mut h = host(session);
    h.launch(0).unwrap();
    h.advance_micros(0).unwrap();
    let count = h.output_count();
    let clips = h.engine.session().slots.clone();
    for column in [1, 3, 5, 7, 8, 9, 10, 11, 6] {
        pad(&mut h, column, true);
        pad(&mut h, column, false);
    }
    assert_eq!(h.output_count(), count);
    assert_eq!(h.engine.snapshot().active_voices, 1);
    h.advance_micros(47).unwrap(); // tick 90, release before next loop
    assert_eq!(h.engine.snapshot().active_voices, 0);
    let messages: Vec<_> = h
        .output_since(count)
        .unwrap()
        .iter()
        .map(|m| m.message.as_bytes().to_vec())
        .collect();
    assert!(messages.contains(&vec![0xe1, 25, 65])); // floor(8192 + 0.9 / 48 * 8192)
    assert!(messages.contains(&vec![0xd1, 97]), "{messages:?}"); // nearest 7-bit value to 50000/65535
    assert!(messages.contains(&vec![0xb1, 74, 39])); // nearest 7-bit value to 20000/65535
    assert!(messages.contains(&vec![0x81, 60, 17]));
    assert_eq!(h.engine.session().slots, clips);
}
