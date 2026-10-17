use crest_linnstrument::{MpeOutput, MpeZone};
use crest_performance_core::*;
use crest_performance_testkit::CaptureSink;

fn event(id: u64, at: u64, gesture: Gesture) -> InputEvent {
    InputEvent {
        at: BeatTime(at),
        note: NoteKey {
            source: SourceId(1),
            track: TrackId(1),
            note: NoteId(id),
        },
        gesture,
    }
}
fn on() -> Gesture {
    Gesture::On {
        key: 60,
        velocity: 93,
        expression: Expression::default(),
    }
}
fn expression() -> Gesture {
    Gesture::Expression {
        expression: Expression {
            pitch: 543_210,
            pressure: 43_210,
            timbre: 12_345,
        },
    }
}
fn clip() -> Clip {
    let mut events = vec![];
    for (id, start, duration, closing) in [
        (1, 149, 300, true),
        (2, 150, 250, true),
        (3, 850, 100, false),
        (4, 850, 100, true),
        (5, 950, 0, true),
    ] {
        events.push(event(id, start, on()));
        if closing {
            events.push(event(id, start + duration, expression()));
        }
        events.push(event(id, start + duration, Gesture::Off { velocity: 17 }));
    }
    events.sort_by_key(|event| event.at);
    Clip {
        length: BeatTime(1000),
        events,
        harmony_origin: vec![HarmonyGesture {
            algorithm_version: HARMONY_ALGORITHM_VERSION,
            input: event(1, 149, on()),
            settings: HarmonySettings::default(),
        }],
        pattern_origin: vec![],
        follow: None,
        conditions: vec![ConditionalNote {
            note: event(2, 0, on()).note,
            condition: NoteCondition::Every {
                period: 2,
                offset: 0,
            },
        }],
    }
}
fn engine(clip: Clip) -> Engine {
    let mut session = Session::default();
    session.slots[0].clip = Some(clip);
    Engine::new(API_VERSION, session, Limits::default()).unwrap()
}
fn quantize(engine: &mut Engine, ticks: u64) -> Result<()> {
    engine.command(Command::QuantizeClip {
        slot: 0,
        quantum: BeatTime(ticks),
    })
}

#[test]
fn nearest_feasible_attack_grid_preserves_durations_expression_conditions_and_intent() {
    let original = clip();
    let result = original.quantize_attacks(BeatTime(100)).unwrap();
    let mut notes = result.notes().unwrap();
    notes.sort_by_key(|note| note.id);
    assert_eq!(
        notes.iter().map(|note| note.start.0).collect::<Vec<_>>(),
        [100, 200, 900, 800, 900]
    );
    for (before, mut after) in original.notes().unwrap().into_iter().zip(notes) {
        after.start = before.start;
        assert_eq!(after, before, "only attack position changes");
    }
    assert_eq!(result.conditions, original.conditions);
    assert_eq!(result.harmony_origin, original.harmony_origin);
    assert_eq!(result.events.len(), original.events.len());
    assert!(
        result
            .events
            .iter()
            .filter(|event| event.at == result.length)
            .all(|event| matches!(event.gesture, Gesture::Off { .. }))
    );
    assert_eq!(result.quantize_attacks(BeatTime(100)).unwrap(), result);
}

#[test]
fn explicit_edit_undo_no_op_and_errors_preserve_the_complete_previous_clip() {
    let original = clip();
    let mut e = engine(original.clone());
    quantize(&mut e, 100).unwrap();
    let changed = e.save_session().unwrap();
    quantize(&mut e, 100).unwrap(); // no-op must not replace Undo
    assert!(quantize(&mut e, 0).is_err());
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(0),
    })
    .unwrap();
    assert!(quantize(&mut e, 10).is_err());
    assert_eq!(e.save_session().unwrap(), changed);
    e.command(Command::Stop).unwrap();
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().slots[0].clip.as_ref().unwrap(), &original);
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    assert!(quantize(&mut e, 100).is_err());
    e.command(Command::CancelRecord).unwrap();
    assert_eq!(e.session().slots[0].clip.as_ref().unwrap(), &original);
}

#[test]
fn grids_and_bounds_at_u64_limit_do_not_wrap_or_drop_expression() {
    let mut original = clip();
    original.length = BeatTime(u64::MAX);
    original.harmony_origin.clear();
    original.conditions.clear();
    original.events = vec![
        event(1, u64::MAX - 100, on()),
        event(1, u64::MAX - 50, expression()),
        event(1, u64::MAX, Gesture::Off { velocity: 0 }),
    ];
    assert_eq!(original.quantize_attacks(BeatTime(1)).unwrap(), original);
    let changed = original.quantize_attacks(BeatTime(u64::MAX)).unwrap();
    assert_eq!(
        changed
            .events
            .iter()
            .map(|event| event.at.0)
            .collect::<Vec<_>>(),
        [0, 50, 100]
    );
    assert_eq!(changed.events[1].gesture, expression());
    assert!(original.quantize_attacks(BeatTime(0)).is_err());
    let mut malformed = original;
    malformed.events[1].at = BeatTime(u64::MAX);
    assert!(malformed.quantize_attacks(BeatTime(100)).is_err());
}

#[test]
fn reused_identities_reject_atomically_instead_of_merging_lifetimes() {
    let mut original = clip();
    original.conditions.clear();
    original.events = vec![
        event(1, 10, on()),
        event(1, 20, Gesture::Off { velocity: 0 }),
        event(1, 30, on()),
        event(1, 40, Gesture::Off { velocity: 0 }),
    ];
    original.validate(TrackId(1)).unwrap();
    let mut e = engine(original.clone());
    assert_eq!(
        quantize(&mut e, 100),
        Err(Error::Invalid("reused note identity inside clip"))
    );
    assert_eq!(e.session().slots[0].clip.as_ref().unwrap(), &original);
}

#[test]
fn releases_precede_new_zero_duration_lifetimes_at_the_same_grid_point() {
    let mut original = clip();
    original.length = BeatTime(300);
    original.conditions.clear();
    original.events = vec![
        event(1, 0, on()),
        event(2, 150, on()),
        event(2, 150, expression()),
        event(2, 150, Gesture::Off { velocity: 0 }),
        event(1, 200, expression()),
        event(1, 200, Gesture::Off { velocity: 0 }),
    ];
    let changed = original.quantize_attacks(BeatTime(100)).unwrap();
    assert_eq!(
        changed
            .events
            .iter()
            .map(|event| event.note.note.0)
            .collect::<Vec<_>>(),
        [1, 1, 1, 2, 2, 2]
    );
    let mut session = Session::default();
    session.slots[0].clip = Some(changed);
    let mut e = Engine::new(
        API_VERSION,
        session,
        Limits {
            voices: 1,
            output_events: 16,
            recorded_events: 16,
        },
    )
    .unwrap();
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(0),
    })
    .unwrap();
    e.advance(BeatTime(299)).unwrap();
    assert_eq!(e.snapshot().active_voices, 0);
}

#[test]
fn reopened_quantized_clip_keeps_mpe_ownership_beside_same_pitch_live_note() {
    let mut e = engine(clip());
    quantize(&mut e, 100).unwrap();
    let saved = e.save_session().unwrap();
    let json = serde_json::to_string(&saved).unwrap();
    let mut e = Engine::new(
        API_VERSION,
        serde_json::from_str(&json).unwrap(),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(e.save_session().unwrap(), saved);
    let mut sink = CaptureSink::default();
    let mut output = MpeOutput::new(DestinationId(1), MpeZone::lower());
    let live = event(99, 0, on());
    e.input(live).unwrap();
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(0),
    })
    .unwrap();
    e.advance(BeatTime(1999)).unwrap();
    e.command(Command::Stop).unwrap();
    sink.collect(&mut e, &mut output).unwrap();
    let attacks: Vec<_> = sink
        .events
        .iter()
        .filter(|event| matches!(event.gesture, Gesture::On { .. }))
        .collect();
    assert_eq!(
        attacks.iter().map(|event| event.at.0).collect::<Vec<_>>(),
        [0, 100, 200, 800, 900, 900, 1100, 1800, 1900, 1900]
    );
    for attack in &attacks {
        let releases: Vec<_> = sink
            .events
            .iter()
            .filter(|event| {
                event.voice == attack.voice && matches!(event.gesture, Gesture::Off { .. })
            })
            .collect();
        assert_eq!(releases.len(), 1);
        if attack.at.0 == 0 {
            assert_eq!(releases[0].at, BeatTime(1999));
        }
    }
    assert!(
        sink.events
            .iter()
            .filter(|event| matches!(event.gesture, Gesture::Expression { .. }))
            .all(|event| event.voice != attacks[0].voice && event.gesture == expression())
    );
    assert_eq!(e.snapshot().active_voices, 0);
}
