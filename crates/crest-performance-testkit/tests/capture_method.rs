use crest_performance_core::*;

#[test]
fn host_cutoff_method_retains_input_time_ownership_and_undo() {
    let mut e = Engine::new(API_VERSION, Session::default(), Limits::default()).unwrap();
    let note = NoteKey {
        source: SourceId(1),
        track: TrackId(1),
        note: NoteId(1),
    };
    e.advance(BeatTime(7)).unwrap();
    e.input(InputEvent {
        at: BeatTime(7),
        note,
        gesture: Gesture::On {
            key: 60,
            velocity: 90,
            expression: Expression::default(),
        },
    })
    .unwrap();
    e.advance(BeatTime(19)).unwrap();
    let expression = Expression {
        pitch: -300_000,
        pressure: 55_000,
        timbre: 60_000,
    };
    e.input(InputEvent {
        at: BeatTime(19),
        note,
        gesture: Gesture::Expression { expression },
    })
    .unwrap();
    e.capture_recent_at(0, BeatTime(100), BeatTime(100))
        .unwrap();
    assert_eq!(e.position(), BeatTime(19));
    assert_eq!(e.snapshot().active_voices, 1);
    assert_eq!(e.drain_output().count(), 2);
    let before = e.save_session().unwrap();
    let captured = before.slots[0].clip.as_ref().unwrap().notes().unwrap();
    assert_eq!(
        (captured[0].start, captured[0].duration),
        (BeatTime(7), BeatTime(93))
    );
    assert_eq!(
        captured[0].expression.points.last().unwrap().value,
        expression
    );
    for (slot, length, cutoff) in [(0, 100, 19), (0, 0, 100), (0, 101, 100), (99, 100, 100)] {
        assert!(
            e.capture_recent_at(slot, BeatTime(length), BeatTime(cutoff))
                .is_err()
        );
        assert_eq!(e.save_session().unwrap(), before);
    }
    e.advance(BeatTime(20)).unwrap();
    e.input(InputEvent {
        at: BeatTime(20),
        note,
        gesture: Gesture::Off { velocity: 17 },
    })
    .unwrap();
    assert_eq!(e.drain_output().count(), 1);
    assert_eq!(e.snapshot().active_voices, 0);
    e.command(Command::Undo).unwrap();
    assert!(e.session().slots[0].clip.is_none());
}
