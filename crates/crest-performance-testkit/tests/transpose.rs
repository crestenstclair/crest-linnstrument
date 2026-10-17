use crest_linnstrument::{MpeOutput, MpeZone};
use crest_performance_core::*;

fn note(key: u8, at: u64, gesture: Gesture) -> InputEvent {
    InputEvent {
        at: BeatTime(at),
        note: NoteKey {
            source: SourceId(1),
            track: TrackId(1),
            note: NoteId(u64::from(key)),
        },
        gesture,
    }
}
fn on(key: u8) -> Gesture {
    Gesture::On {
        key,
        velocity: 90,
        expression: Expression::default(),
    }
}
fn session() -> Session {
    let mut session = Session::default();
    for (slot, key) in [(0, 60), (1, 67)] {
        session.slots[slot].clip = Some(Clip {
            length: BeatTime(20),
            events: vec![
                note(key, 0, on(key)),
                note(
                    key,
                    5,
                    Gesture::Expression {
                        expression: Expression {
                            pitch: 234_567,
                            pressure: 12_345,
                            timbre: 54_321,
                        },
                    },
                ),
                note(key, 15, Gesture::Off { velocity: 8 }),
            ],
            harmony_origin: vec![],
            pattern_origin: vec![],
            follow: None,
            conditions: vec![],
            parameter_locks: vec![],
        });
    }
    session
}
fn engine() -> Engine {
    Engine::new(API_VERSION, session(), Limits::default()).unwrap()
}
fn shift(e: &mut Engine, slot: usize, semitones: i8) -> Result<()> {
    e.command(Command::SetClipTranspose { slot, semitones })
}
fn launch(e: &mut Engine, slot: usize) {
    e.command(Command::Launch {
        slot,
        quantum: BeatTime(0),
    })
    .unwrap();
}

#[test]
fn future_attacks_shift_but_held_clip_and_live_voices_keep_expression_and_wire_release_pitch() {
    let mut e = engine();
    let saved = e.save_session().unwrap();
    let mut midi = MpeOutput::new(DestinationId(1), MpeZone::lower());
    let live = InputEvent {
        note: NoteKey {
            source: SourceId(2),
            ..note(60, 0, on(60)).note
        },
        ..note(60, 0, on(60))
    };
    e.input(live).unwrap();
    launch(&mut e, 0);
    launch(&mut e, 1);
    e.advance(BeatTime(0)).unwrap();
    let initial: Vec<_> = e.drain_output().collect();
    assert_eq!(
        initial.iter().map(|e| e.gesture).collect::<Vec<_>>(),
        vec![on(60), on(60), on(67)]
    );
    for event in &initial {
        midi.accept(*event).unwrap();
    }
    shift(&mut e, 0, 12).unwrap();
    assert_eq!(e.drain_output().count(), 0, "held voices must not retune");
    e.advance(BeatTime(20)).unwrap();
    let changed: Vec<_> = e.drain_output().collect();
    assert_eq!(changed[0].voice, initial[1].voice);
    assert_eq!(
        changed[0].gesture,
        note(
            60,
            5,
            Gesture::Expression {
                expression: Expression {
                    pitch: 234_567,
                    pressure: 12_345,
                    timbre: 54_321
                }
            }
        )
        .gesture
    );
    assert_eq!(changed[4].gesture, on(72));
    assert_eq!(changed[5].gesture, on(67));
    assert_ne!(changed[4].voice, initial[1].voice);
    for event in &changed {
        midi.accept(*event).unwrap();
    }
    shift(&mut e, 0, 0).unwrap();
    e.advance(BeatTime(40)).unwrap();
    let restored: Vec<_> = e.drain_output().collect();
    assert_eq!(restored[0].voice, changed[4].voice);
    assert_eq!(restored[4].gesture, on(60));
    let mut releases = vec![];
    for event in restored {
        let batch = midi.accept(event).unwrap();
        releases.extend(
            batch
                .messages()
                .iter()
                .filter(|m| m.bytes()[0] & 0xf0 == 0x80)
                .map(|m| m.bytes()[1]),
        );
    }
    assert_eq!(releases, vec![72, 67]);
    assert_eq!(e.save_session().unwrap(), saved);
    e.command(Command::Stop).unwrap();
    assert_eq!(e.snapshot().active_voices, 0);
}

#[test]
fn range_rejection_is_atomic_and_late_release_is_safe() {
    let mut s = session();
    s.slots[0].clip.as_mut().unwrap().events[0].gesture = on(120);
    let mut e = Engine::new(API_VERSION, s, Limits::default()).unwrap();
    assert!(shift(&mut e, 0, 12).is_err());
    shift(&mut e, 0, 0).unwrap();
    launch(&mut e, 0);
    shift(&mut e, 0, -12).unwrap();
    assert!(matches!(shift(&mut e, 0, 12), Err(Error::Invalid(_))));
    assert_eq!(e.snapshot().clip_transpositions[0], -12);
    assert_eq!(e.drain_output().count(), 0);
    e.advance(BeatTime(0)).unwrap();
    assert_eq!(e.drain_output().next().unwrap().gesture, on(108));
    e.command(Command::StopSlot { slot: 0 }).unwrap();
    shift(&mut e, 0, 0).unwrap();
    assert_eq!(e.snapshot().clip_transpositions[0], 0);
    assert!(matches!(shift(&mut e, usize::MAX, 0), Err(Error::NotFound)));
}

#[test]
fn mode_disconnect_relaunch_and_reopen_restore_original_without_changing_undo() {
    let mut e = engine();
    e.command(Command::Duplicate { from: 0, to: 2 }).unwrap();
    let saved = e.save_session().unwrap();
    launch(&mut e, 0);
    shift(&mut e, 0, 12).unwrap();
    e.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
    assert_eq!(e.snapshot().clip_transpositions[0], 0);
    shift(&mut e, 0, -12).unwrap();
    e.disconnect(SourceId(1)).unwrap();
    assert_eq!(e.snapshot().clip_transpositions[0], 0);
    shift(&mut e, 0, 12).unwrap();
    launch(&mut e, 0);
    assert_eq!(e.snapshot().clip_transpositions[0], 0);
    shift(&mut e, 0, -12).unwrap();
    assert_eq!(e.save_session().unwrap(), saved);
    let reopened = Engine::new(API_VERSION, saved, Limits::default()).unwrap();
    assert!(
        reopened
            .snapshot()
            .clip_transpositions
            .iter()
            .all(|s| *s == 0)
    );
    e.command(Command::Undo).unwrap();
    assert!(
        e.session().slots[2].clip.is_none(),
        "temporary changes never consume clip Undo"
    );
    e.command(Command::Stop).unwrap();
    assert!(e.snapshot().clip_transpositions.iter().all(|s| *s == 0));
}

#[test]
fn follower_conditions_and_locks_resolve_before_transposed_attack_ownership() {
    let mut s = session();
    let clip = s.slots[0].clip.as_mut().unwrap();
    clip.follow = Some(HarmonyFollow {
        reference: s.harmony,
    });
    let id = clip.events[0].note;
    *clip = clip
        .with_note_condition(id, NoteCondition::Fill {})
        .unwrap()
        .with_note_parameter(id, 4, Some(54_321))
        .unwrap();
    let mut e = Engine::new(API_VERSION, s, Limits::default()).unwrap();
    e.set_note_parameter_capabilities(&[ParameterAddress {
        destination: DestinationId(1),
        parameter: 4,
    }])
    .unwrap();
    e.command(Command::SetHarmony(HarmonySettings {
        degree: 4,
        ..HarmonySettings::default()
    }))
    .unwrap();
    e.command(Command::SetHarmonySource(HarmonySource::Selected {}))
        .unwrap();
    launch(&mut e, 0);
    shift(&mut e, 0, 12).unwrap();
    e.advance(BeatTime(19)).unwrap();
    assert_eq!(e.drain_output().count(), 0);
    assert_eq!(e.drain_parameter_output().count(), 0);
    e.command(Command::SetFill(true)).unwrap();
    e.advance(BeatTime(20)).unwrap();
    let attack = e.drain_output().next().unwrap();
    assert_eq!(attack.gesture, on(79)); // C follows G, then moves up one octave.
    let parameter = e.drain_parameter_output().next().unwrap();
    assert_eq!(
        (parameter.at, parameter.voice, parameter.value),
        (BeatTime(20), Some(attack.voice), 54_321)
    );
    e.command(Command::SetClipMuted {
        slot: 0,
        muted: true,
    })
    .unwrap();
    assert!(matches!(
        e.drain_output().next().unwrap().gesture,
        Gesture::Off { .. }
    ));
    e.advance(BeatTime(40)).unwrap();
    assert_eq!(e.drain_output().count(), 0);
    assert_eq!(e.drain_parameter_output().count(), 0);
}

#[test]
fn bounded_output_retry_keeps_exact_pitch_time_and_release_capacity() {
    let mut e = Engine::new(
        API_VERSION,
        session(),
        Limits {
            voices: 1,
            output_events: 2,
            ..Limits::default()
        },
    )
    .unwrap();
    launch(&mut e, 0);
    shift(&mut e, 0, 12).unwrap();
    assert!(matches!(e.advance(BeatTime(20)), Err(Error::Capacity(_))));
    let first: Vec<_> = e.drain_output().collect();
    assert_eq!(first[0].gesture, on(72));
    assert_eq!(e.position(), BeatTime(5));
    assert_eq!(first.len(), 2);
    assert!(matches!(first[1].gesture, Gesture::Expression { .. }));
    e.advance(BeatTime(15)).unwrap();
    assert!(matches!(
        e.drain_output().next().unwrap().gesture,
        Gesture::Off { .. }
    ));
    e.advance(BeatTime(20)).unwrap();
    let attack = e.drain_output().next().unwrap();
    assert_eq!((attack.at, attack.gesture), (BeatTime(20), on(72)));
    assert_ne!(attack.voice, first[0].voice);
    e.command(Command::Stop).unwrap();
    assert_eq!(e.snapshot().active_voices, 0);
}

#[test]
fn queued_launch_keeps_boundary_and_later_harmony_range_failure_remains_retryable() {
    let mut s = session();
    s.slots[0].clip.as_mut().unwrap().follow = Some(HarmonyFollow {
        reference: s.harmony,
    });
    let mut e = Engine::new(API_VERSION, s, Limits::default()).unwrap();
    e.advance(BeatTime(3)).unwrap();
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(20),
    })
    .unwrap();
    shift(&mut e, 0, 12).unwrap();
    e.advance(BeatTime(19)).unwrap();
    assert_eq!(e.snapshot().slots[0], SlotState::Queued);
    assert_eq!(e.drain_output().count(), 0);
    e.advance(BeatTime(20)).unwrap();
    let first = e.drain_output().next().unwrap();
    assert_eq!((first.at, first.gesture), (BeatTime(20), on(72)));
    e.advance(BeatTime(35)).unwrap();
    e.drain_output().for_each(drop);
    e.command(Command::SetHarmony(HarmonySettings {
        tonic: 120,
        ..HarmonySettings::default()
    }))
    .unwrap();
    e.command(Command::SetHarmonySource(HarmonySource::Selected {}))
        .unwrap();
    assert!(matches!(e.advance(BeatTime(40)), Err(Error::Invalid(_))));
    assert_eq!(e.position(), BeatTime(35));
    assert_eq!(e.snapshot().active_voices, 0);
    assert_eq!(e.drain_output().count(), 0);
    shift(&mut e, 0, 0).unwrap();
    e.advance(BeatTime(40)).unwrap();
    let retried = e.drain_output().next().unwrap();
    assert_eq!((retried.at, retried.gesture), (BeatTime(40), on(120)));
    assert_eq!(
        retried.voice.0,
        first.voice.0 + 1,
        "rejected attack allocated no voice"
    );
}
