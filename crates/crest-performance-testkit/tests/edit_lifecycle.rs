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

#[test]
fn shortening_a_note_releases_before_an_untouched_zero_duration_note() {
    let mut e = Engine::new(API_VERSION, Session::default(), Limits::default()).unwrap();
    let on = Gesture::On {
        key: 60,
        velocity: 90,
        expression: Expression::default(),
    };
    let expression = Gesture::Expression {
        expression: Expression {
            pitch: 987_654,
            pressure: 4321,
            timbre: 54_321,
        },
    };
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    for input in [
        event(1, 0, on),
        event(1, 100, expression),
        event(2, 200, on),
        event(2, 200, expression),
        event(2, 200, Gesture::Off { velocity: 8 }),
        event(1, 300, Gesture::Off { velocity: 7 }),
    ] {
        e.advance(input.at).unwrap();
        e.input(input).unwrap();
        e.drain_output().for_each(drop);
    }
    e.advance(BeatTime(400)).unwrap();
    e.command(Command::FinishRecord).unwrap();
    let original = e.session().slots[0].clip.clone().unwrap();
    e.command(Command::EditNote {
        slot: 0,
        note: event(1, 0, on).note,
        edit: NoteEdit {
            start: BeatTime(0),
            duration: BeatTime(200),
            key: 60,
        },
    })
    .unwrap();
    let saved = e.save_session().unwrap();
    let edited = saved.slots[0].clip.as_ref().unwrap();
    assert_eq!(
        edited
            .events
            .iter()
            .map(|event| event.note.note.0)
            .collect::<Vec<_>>(),
        [1, 1, 1, 2, 2, 2]
    );
    assert_eq!(edited.notes().unwrap()[1], original.notes().unwrap()[1]);
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().slots[0].clip.as_ref(), Some(&original));
    let reopened: Session = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
    let mut playback = Engine::new(
        API_VERSION,
        reopened,
        Limits {
            voices: 1,
            output_events: 16,
            recorded_events: 16,
        },
    )
    .unwrap();
    playback
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    playback.advance(BeatTime(799)).unwrap();
    let mut sink = CaptureSink::default();
    let mut output = MpeOutput::new(DestinationId(1), MpeZone::new(0, 1, 1, 48).unwrap());
    sink.collect(&mut playback, &mut output).unwrap();
    assert_eq!(sink.events.len(), 12);
    assert_eq!(
        sink.events
            .iter()
            .filter(|event| matches!(event.gesture, Gesture::On { .. }))
            .count(),
        4
    );
    assert_eq!(
        sink.events
            .iter()
            .filter(|event| matches!(event.gesture, Gesture::Off { .. }))
            .count(),
        4
    );
    assert_eq!(playback.snapshot().active_voices, 0);
}
