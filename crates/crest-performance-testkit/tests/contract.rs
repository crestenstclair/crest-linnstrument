use crest_linnstrument::*;
use crest_performance_core::*;
use crest_performance_testkit::*;

fn engine() -> Engine {
    Engine::new(API_VERSION, Session::default(), Limits::default()).unwrap()
}
fn note(id: u64, at: u64, gesture: Gesture) -> InputEvent {
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
        velocity: 100,
        expression: Expression::default(),
    }
}
#[test]
fn independent_expression_survives_session_and_playback_alongside_live_note() {
    let mut engine = engine();
    let mut output = MpeOutput::new(DestinationId(1), MpeZone::lower());
    let mut sink = CaptureSink::default();
    engine.command(Command::BeginRecord { slot: 0 }).unwrap();
    expressive_phrase(&mut engine, &mut sink, &mut output).unwrap();
    assert_eq!(sink.events.len(), 9);
    assert_eq!(
        sink.events[3].gesture,
        Gesture::Expression {
            expression: Expression {
                pitch: 750_000,
                ..Expression::default()
            }
        }
    );
    assert_eq!(
        sink.events[4].gesture,
        Gesture::Expression {
            expression: Expression {
                timbre: 56_762,
                ..Expression::default()
            }
        }
    );
    assert_eq!(
        sink.events[5].gesture,
        Gesture::Expression {
            expression: Expression {
                pressure: 46_442,
                ..Expression::default()
            }
        }
    );
    assert_eq!(sink.events[3].voice, sink.events[0].voice);
    assert_eq!(sink.events[4].voice, sink.events[1].voice);
    assert_eq!(sink.events[5].voice, sink.events[2].voice);
    engine.advance(BeatTime(TICKS_PER_BEAT)).unwrap();
    engine.command(Command::FinishRecord).unwrap();
    let saved = engine.save_session().unwrap();
    let bytes = serde_json::to_vec(&saved).unwrap();
    let reopened: Session = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(saved, reopened);
    let mut replay = Engine::new(API_VERSION, reopened, Limits::default()).unwrap();
    replay.input(note(999, 0, on())).unwrap();
    let mut sink2 = CaptureSink::default();
    let mut out2 = MpeOutput::new(DestinationId(1), MpeZone::lower());
    sink2.collect(&mut replay, &mut out2).unwrap();
    replay
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    replay.advance(BeatTime(TICKS_PER_BEAT - 1)).unwrap();
    sink2.collect(&mut replay, &mut out2).unwrap();
    assert_eq!(sink2.events.len(), 10);
    for (recorded, played) in sink.events.iter().zip(&sink2.events[1..]) {
        assert_eq!(recorded.gesture, played.gesture);
        assert_eq!(recorded.at, played.at);
        assert_ne!(played.voice, sink2.events[0].voice);
    }
    let channels: Vec<_> = sink2
        .midi
        .iter()
        .filter(|m| m.message.bytes()[0] & 0xf0 == 0x90)
        .map(|m| m.message.bytes()[0] & 15)
        .collect();
    assert_eq!(channels, vec![1, 2, 3, 4]);
    replay.command(Command::Stop).unwrap();
    sink2.collect(&mut replay, &mut out2).unwrap();
    assert_eq!(replay.snapshot().active_voices, 0);
}
#[test]
fn running_status_realtime_sysex_and_malformed_recovery() {
    let mut parser = MidiParser::default();
    let bytes = [
        0x7f, 0x91, 60, 0xf8, 100, 64, 80, 0xf0, 1, 2, 0xfa, 3, 0xf7, 65, 100, 0x92, 1, 0x93, 67,
        99,
    ];
    let messages: Vec<_> = bytes
        .into_iter()
        .filter_map(|b| parser.feed(b))
        .map(|m| m.bytes())
        .collect();
    assert_eq!(
        messages,
        vec![
            [0xf8, 0, 0],
            [0x91, 60, 100],
            [0x91, 64, 80],
            [0xfa, 0, 0],
            [0x93, 67, 99]
        ]
    );
}
#[test]
fn destination_exhaustion_is_atomic_and_reuse_resets_expression() {
    let zone = MpeZone::new(0, 1, 1, 48).unwrap();
    let mut out = MpeOutput::new(DestinationId(1), zone);
    let event = OutputEvent {
        at: BeatTime(0),
        destination: DestinationId(1),
        track: TrackId(1),
        voice: VoiceId(1),
        gesture: on(),
    };
    out.accept(event).unwrap();
    assert!(matches!(
        out.accept(OutputEvent {
            voice: VoiceId(2),
            ..event
        }),
        Err(Error::Capacity(_))
    ));
    let batch = out
        .accept(OutputEvent {
            gesture: Gesture::Off { velocity: 17 },
            ..event
        })
        .unwrap();
    assert_eq!(batch.messages()[0].bytes(), [0x81, 60, 17]);
    assert_eq!(batch.messages()[1].bytes(), [0xe1, 0, 64]);
    assert_eq!(batch.messages()[3].bytes(), [0xd1, 0, 0]);
    let reused = out
        .accept(OutputEvent {
            voice: VoiceId(2),
            ..event
        })
        .unwrap();
    assert_eq!(reused.messages().last().unwrap().bytes(), [0x91, 60, 100]);
    let mut independent = MpeOutput::new(DestinationId(2), zone);
    assert!(
        independent
            .accept(OutputEvent {
                destination: DestinationId(2),
                ..event
            })
            .is_ok()
    );
}
#[test]
fn output_pressure_preserves_release_for_retry() {
    let mut e = Engine::new(
        API_VERSION,
        Session::default(),
        Limits {
            voices: 1,
            output_events: 1,
            recorded_events: 4,
        },
    )
    .unwrap();
    e.input(note(1, 0, on())).unwrap();
    assert!(matches!(
        e.input(note(1, 0, Gesture::Off { velocity: 0 })),
        Err(Error::Capacity(_))
    ));
    assert_eq!(e.snapshot().active_voices, 1);
    e.drain_output().for_each(drop);
    e.input(note(1, 0, Gesture::Off { velocity: 0 })).unwrap();
    assert_eq!(e.snapshot().active_voices, 0);
}
#[test]
fn truncates_record_end_and_undo_preserves_previous_clip() {
    let mut e = engine();
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    e.input(note(1, 0, on())).unwrap();
    e.advance(BeatTime(17)).unwrap();
    e.command(Command::FinishRecord).unwrap();
    let clip = e.session().slots[0].clip.as_ref().unwrap();
    assert_eq!(clip.length, BeatTime(17));
    assert_eq!(clip.events.last().unwrap().at, BeatTime(17));
    clip.validate(TrackId(1)).unwrap();
    assert_eq!(e.snapshot().active_voices, 1);
    e.command(Command::Undo).unwrap();
    assert!(e.session().slots[0].clip.is_none());
    e.disconnect(SourceId(1)).unwrap();
    assert_eq!(e.snapshot().active_voices, 0);
}
#[test]
fn versions_invalid_sessions_and_clock_boundaries() {
    assert!(matches!(
        Engine::new(API_VERSION + 1, Session::default(), Limits::default()),
        Err(Error::Version(3))
    ));
    let mut session = Session {
        version: 7,
        ..Session::default()
    };
    assert_eq!(session.validate(), Err(Error::Version(7)));
    session.version = SESSION_VERSION;
    session.slots[0].clip = Some(Clip {
        length: BeatTime(10),
        events: vec![note(1, 0, on())],
        harmony_origin: Vec::new(),
        pattern_origin: Vec::new(),
        follow: None,
    });
    assert!(session.validate().is_err());
    let clock = ClockMapping::new(100, BeatTime(7), 120_000).unwrap();
    assert_eq!(clock.at_micros(500100).unwrap(), BeatTime(960007));
    assert!(clock.at_micros(99).is_err());
}
#[test]
fn led_diff_order_and_reconnect_repaint_are_bounded() {
    let mut renderer = LedRenderer::new(Geometry::Pads128);
    let mut frame = [[0; 8]; 26];
    frame[1][0] = 3;
    let first = renderer.diff(&frame, 1).unwrap();
    assert_eq!(first.len(), 3);
    assert_eq!(first[0].bytes(), [0xb0, 20, 0]);
    assert_eq!(first[1].bytes(), [0xb0, 21, 0]);
    assert_eq!(renderer.diff(&frame, 208).unwrap().len(), 135 * 3);
    assert!(renderer.diff(&frame, 208).unwrap().is_empty());
    renderer.invalidate();
    assert_eq!(renderer.diff(&frame, 208).unwrap().len(), 136 * 3);
    assert_eq!(
        user_pad(MidiMessage::channel(0x90, 3, 12, 100), Geometry::Pads128).unwrap(),
        Some(PadEvent {
            column: 12,
            row: 3,
            pressed: true
        })
    );
}

#[test]
fn edits_duplicates_retrospective_and_overdub_preserve_expression_and_undo() {
    let mut e = engine();
    let mut output = MpeOutput::new(DestinationId(1), MpeZone::lower());
    let mut sink = CaptureSink::default();
    expressive_phrase(&mut e, &mut sink, &mut output).unwrap();
    e.advance(BeatTime(TICKS_PER_BEAT)).unwrap();
    e.command(Command::CaptureRecent {
        slot: 0,
        length: BeatTime(TICKS_PER_BEAT),
    })
    .unwrap();
    let original = e.session().slots[0].clip.clone().unwrap();
    assert_eq!(original.notes().unwrap().len(), 3);
    e.command(Command::Duplicate { from: 0, to: 1 }).unwrap();
    assert_eq!(e.session().slots[1].clip.as_ref(), Some(&original));
    let id = original.notes().unwrap()[0].id;
    e.command(Command::EditNote {
        slot: 1,
        note: id,
        edit: NoteEdit {
            start: BeatTime(123),
            duration: BeatTime(740000),
            key: 61,
        },
    })
    .unwrap();
    let edited = e.session().slots[1].clip.as_ref().unwrap().notes().unwrap();
    let changed = edited.iter().find(|n| n.id == id).unwrap();
    assert_eq!(changed.start, BeatTime(123));
    assert_eq!(changed.expression, original.notes().unwrap()[0].expression);
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().slots[1].clip.as_ref(), Some(&original));
    e.command(Command::BeginOverdub { slot: 0 }).unwrap();
    e.advance(BeatTime(TICKS_PER_BEAT)).unwrap();
    e.input(note(44, TICKS_PER_BEAT, on())).unwrap();
    e.advance(BeatTime(TICKS_PER_BEAT + 50000)).unwrap();
    e.input(note(
        44,
        TICKS_PER_BEAT + 50000,
        Gesture::Off { velocity: 0 },
    ))
    .unwrap();
    e.advance(BeatTime(TICKS_PER_BEAT + 60000)).unwrap();
    e.command(Command::FinishRecord).unwrap();
    let merged = e.session().slots[0].clip.as_ref().unwrap();
    assert_eq!(merged.notes().unwrap().len(), 4);
    assert_eq!(merged.length, original.length);
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().slots[0].clip.as_ref(), Some(&original));
}
#[test]
fn mode_change_keeps_clip_playing_and_release_reserve_survives_full_take() {
    let mut e = Engine::new(
        API_VERSION,
        Session::default(),
        Limits {
            voices: 2,
            output_events: 32,
            recorded_events: 3,
        },
    )
    .unwrap();
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    e.input(note(1, 0, on())).unwrap();
    e.advance(BeatTime(1)).unwrap();
    e.input(note(
        1,
        1,
        Gesture::Expression {
            expression: Expression {
                pitch: 10,
                ..Expression::default()
            },
        },
    ))
    .unwrap();
    assert!(matches!(
        e.input(note(
            1,
            1,
            Gesture::Expression {
                expression: Expression::default()
            }
        )),
        Err(Error::Capacity(_))
    ));
    e.advance(BeatTime(2)).unwrap();
    e.input(note(1, 2, Gesture::Off { velocity: 0 })).unwrap();
    e.command(Command::FinishRecord).unwrap();
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(0),
    })
    .unwrap();
    e.advance(BeatTime(2)).unwrap();
    e.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
    e.command(Command::SetMode(SurfaceMode::Play)).unwrap();
    assert_eq!(e.snapshot().slots[0], SlotState::Playing);
    assert_eq!(e.snapshot().active_voices, 1);
}

#[test]
fn repeated_pitches_keep_distinct_member_ownership_and_reversed_releases() {
    let mut input = NoteInput::mpe(MpeZone::lower(), SourceId(7), TrackId(1));
    let a = input
        .accept(BeatTime(0), MidiMessage::channel(0x90, 1, 60, 100))
        .unwrap()[0];
    let b = input
        .accept(BeatTime(0), MidiMessage::channel(0x90, 2, 60, 100))
        .unwrap()[0];
    assert_ne!(a.note, b.note);
    let pressure = input
        .accept(BeatTime(1), MidiMessage::channel(0xd0, 2, 90, 0))
        .unwrap()[0];
    assert_eq!(pressure.note, b.note);
    let off_b = input
        .accept(BeatTime(2), MidiMessage::channel(0x80, 2, 60, 0))
        .unwrap()[0];
    assert_eq!(off_b.note, b.note);
    let off_a = input
        .accept(BeatTime(3), MidiMessage::channel(0x80, 1, 60, 0))
        .unwrap()[0];
    assert_eq!(off_a.note, a.note);
    assert!(serde_json::from_str::<MidiMessage>(r#"{"bytes":[145,60,100],"len":255}"#).is_err());
}
#[test]
fn a_full_take_does_not_block_release_of_a_note_held_before_recording() {
    let mut e = Engine::new(
        API_VERSION,
        Session::default(),
        Limits {
            voices: 2,
            output_events: 16,
            recorded_events: 2,
        },
    )
    .unwrap();
    e.input(note(1, 0, on())).unwrap();
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    e.input(note(2, 0, on())).unwrap();
    e.advance(BeatTime(1)).unwrap();
    e.input(note(1, 1, Gesture::Off { velocity: 0 })).unwrap();
    assert_eq!(e.snapshot().active_voices, 1);
    e.command(Command::FinishRecord).unwrap();
    e.session().validate().unwrap();
}
