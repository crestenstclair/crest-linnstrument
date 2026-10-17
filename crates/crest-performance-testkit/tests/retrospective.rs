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
fn accept(engine: &mut Engine, event: InputEvent) {
    engine.advance(event.at).unwrap();
    engine.input(event).unwrap();
}
fn capture(slot: usize, length: u64, cutoff: u64) -> Command {
    Command::CaptureRecentAt {
        slot,
        length: BeatTime(length),
        cutoff: BeatTime(cutoff),
    }
}

#[test]
fn explicit_cutoff_captures_latest_xyz_and_elapsed_holds_without_advancing_live_input() {
    let mut engine = Engine::new(API_VERSION, Session::default(), Limits::default()).unwrap();
    for id in 1..=3 {
        accept(&mut engine, event(id, 7, on(60 + id as u8)));
    }
    let changes = [
        Expression {
            pitch: 250_000,
            ..Expression::default()
        },
        Expression {
            timbre: 54_000,
            ..Expression::default()
        },
        Expression {
            pressure: 61_000,
            ..Expression::default()
        },
    ];
    for (index, expression) in changes.into_iter().enumerate() {
        accept(
            &mut engine,
            event(index as u64 + 1, 19, Gesture::Expression { expression }),
        );
    }
    engine.command(capture(0, 100, 100)).unwrap();
    assert_eq!(engine.position(), BeatTime(19));
    assert_eq!(engine.snapshot().active_voices, 3);
    assert_eq!(
        engine.drain_output().count(),
        6,
        "Capture neither emits nor drains musical output"
    );
    let clip = engine.session().slots[0].clip.clone().unwrap();
    let notes = clip.notes().unwrap();
    assert_eq!(notes.len(), 3);
    for (index, note) in notes.iter().enumerate() {
        assert_eq!((note.start, note.duration), (BeatTime(7), BeatTime(93)));
        assert_eq!(note.expression.points.last().unwrap().offset, BeatTime(12));
        assert_eq!(note.expression.points.last().unwrap().value, changes[index]);
    }
    // An already timestamped packet can still be accepted before the copied
    // cutoff. The capture contains only input known when the command ran.
    accept(&mut engine, event(1, 20, Gesture::Off { velocity: 17 }));
    assert_eq!(engine.snapshot().active_voices, 2);
    assert_eq!(engine.session().slots[0].clip.as_ref(), Some(&clip));
    engine.command(Command::Undo).unwrap();
    assert!(engine.session().slots[0].clip.is_none());
    assert_eq!(engine.snapshot().active_voices, 2);
}

#[test]
fn held_note_crossing_capture_start_keeps_initial_expression_and_replays_after_reopen() {
    let mut engine = Engine::new(API_VERSION, Session::default(), Limits::default()).unwrap();
    accept(&mut engine, event(1, 3, on(60)));
    let expression = Expression {
        pitch: -456_789,
        pressure: 31_337,
        timbre: 50_123,
    };
    accept(&mut engine, event(1, 5, Gesture::Expression { expression }));
    engine.command(capture(0, 20, 50)).unwrap();
    let clip = engine.session().slots[0].clip.as_ref().unwrap();
    let notes = clip.notes().unwrap();
    assert_eq!(
        (notes[0].start, notes[0].duration),
        (BeatTime(0), BeatTime(20))
    );
    assert_eq!(notes[0].expression.points[0].value, expression);
    let saved: Session =
        serde_json::from_slice(&serde_json::to_vec(&engine.save_session().unwrap()).unwrap())
            .unwrap();
    let mut replay = Engine::new(API_VERSION, saved, Limits::default()).unwrap();
    replay
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    replay.advance(BeatTime(19)).unwrap();
    let output = replay.drain_output().next().unwrap();
    assert!(matches!(output.gesture, Gesture::On { expression: value, .. } if value == expression));
    replay.command(Command::Stop).unwrap();
    assert!(matches!(
        replay.drain_output().next().unwrap().gesture,
        Gesture::Off { .. }
    ));
}

#[test]
fn invalid_cutoffs_empty_ranges_and_expired_history_leave_existing_clip_unchanged() {
    let mut engine = Engine::new(API_VERSION, Session::default(), Limits::default()).unwrap();
    accept(&mut engine, event(1, 3, on(60)));
    accept(&mut engine, event(1, 5, Gesture::Off { velocity: 0 }));
    engine.command(capture(0, 10, 10)).unwrap();
    let before = engine.save_session().unwrap();
    for command in [
        capture(0, 10, 5),
        capture(0, 0, 10),
        capture(0, 20, 10),
        capture(0, 10, 30),
        capture(99, 10, 10),
    ] {
        assert!(engine.command(command).is_err());
        assert_eq!(engine.save_session().unwrap(), before);
        assert_eq!(engine.position(), BeatTime(5));
    }
    engine.command(Command::Undo).unwrap();
    assert!(engine.session().slots[0].clip.is_none());
}

#[test]
fn event_history_eviction_never_lowers_the_expired_harmony_boundary() {
    let mut engine = Engine::new(
        API_VERSION,
        Session {
            harmony: HarmonySettings {
                expression: ExpressionPolicy::TopVoice,
                articulation: Articulation::Strum {
                    direction: StrumDirection::Ascending,
                    interval: BeatTime(100),
                },
                ..HarmonySettings::default()
            },
            ..Session::default()
        },
        Limits {
            voices: 3,
            output_events: 20,
            recorded_events: 6,
        },
    )
    .unwrap();
    engine
        .command(Command::SetMode(SurfaceMode::Chords))
        .unwrap();
    accept(&mut engine, event(1, 0, on(30)));
    for at in 1..=10 {
        accept(
            &mut engine,
            event(
                1,
                at,
                Gesture::Expression {
                    expression: Expression::default(),
                },
            ),
        );
    }
    accept(&mut engine, event(1, 10, Gesture::Off { velocity: 0 }));
    engine.command(Command::SetMode(SurfaceMode::Play)).unwrap();
    accept(&mut engine, event(2, 11, on(70)));
    for at in 12..=15 {
        accept(
            &mut engine,
            event(
                2,
                at,
                Gesture::Expression {
                    expression: Expression::default(),
                },
            ),
        );
    }
    assert_eq!(
        engine.command(capture(0, 13, 16)),
        Err(Error::Capacity("retrospective history expired"))
    );
    assert!(engine.session().slots[0].clip.is_none());
}
