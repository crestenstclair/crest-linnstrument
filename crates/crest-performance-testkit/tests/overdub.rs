use crest_performance_core::*;

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
fn on(key: u8) -> Gesture {
    Gesture::On {
        key,
        velocity: 96,
        expression: Expression::default(),
    }
}
fn expression() -> Gesture {
    Gesture::Expression {
        expression: Expression {
            pitch: 123_456,
            pressure: 45_678,
            timbre: 23_456,
        },
    }
}
fn clip(events: Vec<InputEvent>) -> Clip {
    Clip {
        length: BeatTime(40),
        events,
        harmony_origin: Vec::new(),
        pattern_origin: Vec::new(),
        follow: None,
        conditions: Vec::new(),
    }
}
fn engine(original: Clip) -> Engine {
    let mut session = Session::default();
    session.slots[0].clip = Some(original);
    Engine::new(API_VERSION, session, Limits::default()).unwrap()
}
fn record(engine: &mut Engine, events: &[InputEvent], finish: u64) {
    engine.command(Command::BeginOverdub { slot: 0 }).unwrap();
    for event in events {
        engine.advance(event.at).unwrap();
        engine.drain_output().for_each(drop);
        engine.input(*event).unwrap();
    }
    engine.advance(BeatTime(finish)).unwrap();
    engine.drain_output().for_each(drop);
    engine.command(Command::FinishRecord).unwrap();
}

#[test]
fn overdub_preserves_original_raw_order_closing_expression_and_zero_duration_notes() {
    let original = clip(vec![
        event(1, 0, on(60)),
        event(2, 0, on(64)),
        event(1, 20, expression()),
        event(1, 20, Gesture::Off { velocity: 7 }),
        event(3, 30, on(67)),
        event(3, 30, expression()),
        event(3, 30, Gesture::Off { velocity: 8 }),
        event(3, 31, on(68)), // A valid raw stream may reuse a released input ID.
        event(3, 32, Gesture::Off { velocity: 8 }),
        event(2, 40, Gesture::Off { velocity: 9 }),
    ]);
    let mut engine = engine(original.clone());
    let take = vec![
        event(9, 5, on(72)),
        event(9, 15, expression()),
        event(9, 15, Gesture::Off { velocity: 10 }),
    ];
    record(&mut engine, &take, 21);
    let merged = engine.session().slots[0].clip.clone().unwrap();
    assert_eq!(merged.length, original.length);
    assert_eq!(
        merged
            .events
            .iter()
            .filter(|e| e.note.note.0 <= 3)
            .copied()
            .collect::<Vec<_>>(),
        original.events
    );
    let mut expected_take = take;
    for event in &mut expected_take {
        event.note.note = NoteId(4);
    }
    assert_eq!(
        merged
            .events
            .iter()
            .filter(|e| e.note.note == NoteId(4))
            .copied()
            .collect::<Vec<_>>(),
        expected_take
    );
    engine.command(Command::Stop).unwrap();
    let saved = engine.save_session().unwrap();
    let reopened: Session = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(reopened, saved);
    let mut playback = Engine::new(API_VERSION, reopened, Limits::default()).unwrap();
    playback
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    playback.advance(BeatTime(40)).unwrap();
    let output: Vec<_> = playback.drain_output().collect();
    // At 40 the next iteration starts after the first iteration's final release.
    for (actual, expected) in output.iter().zip(&merged.events) {
        assert_eq!((actual.at, actual.gesture), (expected.at, expected.gesture));
    }
    assert!(output.len() >= merged.events.len());
    playback.command(Command::Stop).unwrap();
    assert_eq!(playback.snapshot().active_voices, 0);
    engine.command(Command::Undo).unwrap();
    assert_eq!(engine.session().slots[0].clip, Some(original));
}

#[test]
fn overdub_folds_only_new_events_and_clips_expression_at_loop_boundary() {
    let original = clip(vec![
        event(1, 0, on(60)),
        event(1, 40, Gesture::Off { velocity: 0 }),
    ]);
    let mut engine = engine(original.clone());
    record(
        &mut engine,
        &[
            event(9, 10, on(70)),
            event(9, 10, expression()),
            event(9, 10, Gesture::Off { velocity: 11 }),
            event(10, 35, on(71)),
            event(10, 39, expression()),
            event(10, 40, expression()),
            event(9, 45, on(72)), // Reused input identity, new overdub lifetime.
            event(10, 46, expression()),
            event(9, 50, expression()),
            event(9, 50, Gesture::Off { velocity: 12 }),
            event(10, 55, Gesture::Off { velocity: 13 }),
        ],
        56,
    );
    let merged = engine.session().slots[0].clip.as_ref().unwrap();
    let mut expected = original.events;
    expected.extend([
        event(4, 5, on(72)),
        event(2, 10, on(70)),
        event(2, 10, expression()),
        event(2, 10, Gesture::Off { velocity: 11 }),
        event(4, 10, expression()),
        event(4, 10, Gesture::Off { velocity: 12 }),
        event(3, 35, on(71)),
        event(3, 39, expression()),
        event(3, 40, Gesture::Off { velocity: 13 }),
    ]);
    expected.sort_by_key(|e| e.at);
    assert_eq!(merged.events, expected);
    merged.validate(TrackId(1)).unwrap();
}

#[test]
fn overdub_identity_exhaustion_keeps_original_clip_and_cancellable_take() {
    let original = clip(vec![
        event(u64::MAX, 0, on(60)),
        event(u64::MAX, 40, Gesture::Off { velocity: 0 }),
    ]);
    let mut engine = engine(original.clone());
    engine.command(Command::BeginOverdub { slot: 0 }).unwrap();
    engine.input(event(1, 0, on(72))).unwrap();
    engine
        .input(event(1, 0, Gesture::Off { velocity: 0 }))
        .unwrap();
    engine.advance(BeatTime(1)).unwrap();
    assert_eq!(
        engine.command(Command::FinishRecord),
        Err(Error::Capacity("clip note identities"))
    );
    assert_eq!(engine.session().slots[0].clip, Some(original.clone()));
    assert_eq!(engine.snapshot().slots[0], SlotState::Overdubbing);
    engine.command(Command::CancelRecord).unwrap();
    engine.command(Command::Stop).unwrap();
    assert_eq!(engine.snapshot().active_voices, 0);
    assert_eq!(engine.session().slots[0].clip, Some(original));
}
