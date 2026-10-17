use crest_performance_core::*;
use std::collections::BTreeMap;

fn clip(slot: usize) -> Clip {
    let note = NoteKey {
        source: SourceId(1),
        track: TrackId(1),
        note: NoteId(1),
    };
    let mut events = vec![InputEvent {
        at: BeatTime(7),
        note,
        gesture: Gesture::On {
            key: 60 + slot as u8,
            velocity: 90,
            expression: Expression::default(),
        },
    }];
    for sample in 0..2_000 {
        events.push(InputEvent {
            at: BeatTime(10 + sample * 3),
            note,
            gesture: Gesture::Expression {
                expression: Expression {
                    pitch: (sample as i32 - 1_000) * 123,
                    pressure: (sample * 31) as u16,
                    timbre: (65_535 - sample * 29) as u16,
                },
            },
        });
    }
    events.push(InputEvent {
        at: BeatTime(6_009),
        note,
        gesture: Gesture::Off { velocity: 35 },
    });
    Clip {
        length: BeatTime(10_000 + slot as u64 * 1_000),
        events,
        harmony_origin: vec![],
        pattern_origin: vec![],
        follow: None,
        conditions: Vec::new(),
        parameter_locks: Vec::new(),
    }
}

fn expected_iterations(slot: usize, clip: &Clip, end: BeatTime) -> Vec<(usize, InputEvent)> {
    (0..=end.0 / clip.length.0)
        .flat_map(|cycle| iteration(slot, clip, cycle * clip.length.0, end))
        .collect()
}

fn iteration(slot: usize, clip: &Clip, offset: u64, end: BeatTime) -> Vec<(usize, InputEvent)> {
    clip.events
        .iter()
        .copied()
        .filter_map(|mut event| {
            event.at.0 += offset;
            (event.at <= end).then_some((slot, event))
        })
        .collect()
}

#[test]
fn large_expressive_bank_round_trips_and_retries_without_losing_events_or_ownership() {
    let mut session = Session::default();
    for (slot, entry) in session.slots.iter_mut().enumerate() {
        entry.clip = Some(clip(slot));
    }
    let bytes = serde_json::to_vec_pretty(&session).unwrap();
    assert!(bytes.len() > 4_000_000 && bytes.len() < 8_000_000);
    let restored: Session = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(session, restored);
    let end = BeatTime(80_000);
    let mut expected: Vec<_> = restored
        .slots
        .iter()
        .enumerate()
        .flat_map(|(slot, entry)| expected_iterations(slot, entry.clip.as_ref().unwrap(), end))
        .collect();
    // Independent oracle follows the public ordering: timestamp, releases first,
    // then slot order. Equal-time events within one clip keep their stored order.
    expected.sort_by_key(|(slot, event)| {
        (
            event.at,
            !matches!(event.gesture, Gesture::Off { .. }),
            *slot,
        )
    });
    let mut engine = Engine::new(
        API_VERSION,
        restored,
        Limits {
            voices: 8,
            output_events: 16,
            recorded_events: 2_048,
        },
    )
    .unwrap();
    for slot in 0..8 {
        engine
            .command(Command::Launch {
                slot,
                quantum: BeatTime(0),
            })
            .unwrap();
    }
    let mut output = vec![];
    let mut retries = 0;
    loop {
        let result = engine.advance(end);
        output.extend(engine.drain_output());
        match result {
            Ok(()) => break,
            Err(Error::Capacity(_)) => retries += 1,
            other => panic!("unexpected scheduler result: {other:?}"),
        }
    }
    assert!(retries > 1_000);
    assert_eq!(output.len(), expected.len());
    assert!(output.len() > 90_000);
    verify_ownership(&output, &expected);
    let held = engine.snapshot().active_voices;
    engine.command(Command::Stop).unwrap();
    let releases: Vec<_> = engine.drain_output().collect();
    assert_eq!(releases.len(), held);
    assert!(
        releases
            .iter()
            .all(|e| matches!(e.gesture, Gesture::Off { .. }))
    );
    assert_eq!(engine.snapshot().active_voices, 0);
}

fn verify_ownership(output: &[OutputEvent], expected: &[(usize, InputEvent)]) {
    let mut held = BTreeMap::new();
    let mut previous_voice = 0;
    for (actual, (slot, wanted)) in output.iter().zip(expected) {
        assert_eq!((actual.at, actual.gesture), (wanted.at, wanted.gesture));
        assert_eq!(actual.destination, DestinationId(1));
        match actual.gesture {
            Gesture::On { .. } => {
                assert!(actual.voice.0 > previous_voice);
                previous_voice = actual.voice.0;
                assert!(held.insert(slot, actual.voice).is_none());
            }
            Gesture::Expression { .. } => assert_eq!(held.get(slot), Some(&actual.voice)),
            Gesture::Off { .. } => assert_eq!(held.remove(slot), Some(actual.voice)),
        }
    }
}

fn record_overlay(engine: &mut Engine, round: u64) {
    engine.command(Command::BeginOverdub { slot: 0 }).unwrap();
    let start = engine.position().0;
    let offset = 13 + round * 7;
    let note = NoteKey {
        source: SourceId(1),
        track: TrackId(1),
        note: NoteId(100 + round),
    };
    let gestures = [
        Gesture::On {
            key: 60,
            velocity: 80,
            expression: Expression::default(),
        },
        Gesture::Expression {
            expression: Expression {
                pitch: 234_567,
                pressure: 12_345,
                timbre: 54_321,
            },
        },
        Gesture::Expression {
            expression: Expression {
                pitch: -123_456,
                pressure: 54_321,
                timbre: 12_345,
            },
        },
        Gesture::Off { velocity: 27 },
    ];
    for (index, gesture) in gestures.into_iter().enumerate() {
        let at = BeatTime(start + offset + index as u64 * 17);
        engine.advance(at).unwrap();
        engine.drain_output().for_each(drop);
        engine.input(InputEvent { at, note, gesture }).unwrap();
        engine.drain_output().for_each(drop);
    }
    engine.command(Command::FinishRecord).unwrap();
    engine.drain_output().for_each(drop);
}

#[test]
fn repeated_overdubs_preserve_free_timing_expression_and_last_complete_undo() {
    let mut session = Session::default();
    let original = clip(0);
    session.slots[0].clip = Some(original.clone());
    let mut engine = Engine::new(API_VERSION, session, Limits::default()).unwrap();
    let original_note = original.notes().unwrap().remove(0);
    for round in 0..128 {
        let before = engine.session().slots[0].clip.clone();
        record_overlay(&mut engine, round);
        let merged = engine.session().slots[0].clip.as_ref().unwrap();
        let notes = merged.notes().unwrap();
        assert_eq!(merged.length, original.length);
        assert_eq!(notes.len(), round as usize + 2);
        assert_eq!(notes[0], original_note);
        let last = notes.last().unwrap();
        assert_eq!(last.start, BeatTime(13 + round * 7));
        assert_eq!(last.duration, BeatTime(51));
        assert_eq!(last.release_velocity, 27);
        assert_eq!(last.expression.points[1].offset, BeatTime(17));
        assert_eq!(last.expression.points[1].value.pitch, 234_567);
        assert_eq!(last.expression.points[2].offset, BeatTime(34));
        assert_eq!(last.expression.points[2].value.pressure, 54_321);
        if round == 127 {
            engine.command(Command::Undo).unwrap();
            assert_eq!(engine.session().slots[0].clip, before);
        }
    }
    engine.command(Command::Stop).unwrap();
    engine.drain_output().for_each(drop);
    assert_eq!(engine.snapshot().active_voices, 0);
    let saved = engine.save_session().unwrap();
    let restored: Session = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(saved, restored);
}
