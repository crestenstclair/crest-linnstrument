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
fn accept(engine: &mut Engine, event: InputEvent) {
    engine.advance(event.at).unwrap();
    engine.input(event).unwrap();
}
fn expression(pitch: i32, pressure: u16, timbre: u16) -> Gesture {
    Gesture::Expression {
        expression: Expression {
            pitch,
            pressure,
            timbre,
        },
    }
}

#[test]
fn interrupted_take_snapshot_closes_only_its_copy_and_recording_continues_losslessly() {
    let mut engine = Engine::new(API_VERSION, Session::default(), Limits::default()).unwrap();
    engine.command(Command::BeginRecord { slot: 0 }).unwrap();
    for id in 1..=3 {
        accept(
            &mut engine,
            event(
                id,
                0,
                Gesture::On {
                    key: 60 + id as u8,
                    velocity: 100,
                    expression: Expression::default(),
                },
            ),
        );
    }
    accept(&mut engine, event(1, 1, expression(123_456, 111, 222)));
    accept(&mut engine, event(2, 2, Gesture::Off { velocity: 17 }));
    accept(&mut engine, event(3, 2, expression(0, 65535, 333)));
    let checkpoint = engine.checkpoint_session().unwrap();
    let timed = engine.checkpoint_session_at(BeatTime(12)).unwrap();
    let timed_clip = timed.slots[0].clip.as_ref().unwrap();
    assert_eq!(timed_clip.length, BeatTime(12));
    assert_eq!(timed_clip.events.last().unwrap().at, BeatTime(12));
    assert!(engine.checkpoint_session_at(BeatTime(2)).is_err());
    assert_eq!(engine.position(), BeatTime(2));
    assert_eq!(engine.snapshot().active_voices, 2);
    assert_eq!(engine.snapshot().slots[0], SlotState::Recording);
    assert!(engine.save_session().is_err());
    let emitted: Vec<_> = engine.drain_output().collect();
    assert_eq!(
        emitted.len(),
        6,
        "Snapshot must not emit MIDI or drain queued output"
    );
    assert_eq!(
        emitted
            .iter()
            .filter(|e| matches!(e.gesture, Gesture::Off { .. }))
            .count(),
        1
    );
    let clip = checkpoint.slots[0].clip.as_ref().unwrap();
    assert_eq!(clip.length, BeatTime(3));
    assert_eq!(
        &clip.events[6..],
        &[
            event(1, 3, Gesture::Off { velocity: 0 }),
            event(3, 3, Gesture::Off { velocity: 0 })
        ]
    );
    assert_eq!(clip.events[3], event(1, 1, expression(123_456, 111, 222)));
    assert_eq!(clip.events[5], event(3, 2, expression(0, 65535, 333)));
    let reopened: Session =
        serde_json::from_slice(&serde_json::to_vec(&checkpoint).unwrap()).unwrap();
    assert_eq!(reopened, checkpoint);
    let mut recovered = Engine::new(API_VERSION, reopened, Limits::default()).unwrap();
    assert_eq!(recovered.snapshot().active_voices, 0);
    recovered
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    recovered.advance(BeatTime(2)).unwrap();
    recovered.command(Command::Stop).unwrap();
    assert_eq!(recovered.snapshot().active_voices, 0);
    accept(&mut engine, event(1, 4, expression(456_789, 444, 555)));
    accept(&mut engine, event(1, 4, Gesture::Off { velocity: 30 }));
    accept(&mut engine, event(3, 5, Gesture::Off { velocity: 40 }));
    engine.advance(BeatTime(6)).unwrap();
    engine.command(Command::FinishRecord).unwrap();
    let finished = engine.save_session().unwrap();
    let final_clip = finished.slots[0].clip.as_ref().unwrap();
    assert_eq!(final_clip.length, BeatTime(6));
    assert_eq!(
        final_clip.events[6],
        event(1, 4, expression(456_789, 444, 555))
    );
    assert_eq!(&final_clip.events[..6], &clip.events[..6]);
    assert_eq!(engine.snapshot().active_voices, 0);
}

#[test]
fn empty_take_snapshot_keeps_committed_state_and_exhausted_clock_is_explicit() {
    let mut engine = Engine::new(API_VERSION, Session::default(), Limits::default()).unwrap();
    let original = engine.save_session().unwrap();
    engine.command(Command::BeginRecord { slot: 0 }).unwrap();
    assert_eq!(engine.checkpoint_session().unwrap(), original);
    accept(
        &mut engine,
        event(
            1,
            u64::MAX,
            Gesture::On {
                key: 60,
                velocity: 100,
                expression: Expression::default(),
            },
        ),
    );
    assert!(engine.checkpoint_session().is_err());
    assert_eq!(engine.snapshot().active_voices, 1);
    assert_eq!(engine.snapshot().slots[0], SlotState::Recording);
}
