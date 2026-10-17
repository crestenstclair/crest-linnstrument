use crest_performance_core::*;

fn note(id: u64) -> NoteKey {
    NoteKey {
        source: SourceId(3),
        track: TrackId(1),
        note: NoteId(id),
    }
}
fn event(at: u64, id: u64, gesture: Gesture) -> InputEvent {
    InputEvent {
        at: BeatTime(at),
        note: note(id),
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
fn phrase() -> Clip {
    Clip {
        length: BeatTime(100),
        events: vec![
            event(10, 1, on()),
            event(10, 2, on()),
            event(10, 3, on()),
            event(
                20,
                1,
                Gesture::Expression {
                    expression: Expression {
                        pitch: 500_000,
                        pressure: 60_000,
                        timbre: 12_000,
                    },
                },
            ),
            event(30, 2, Gesture::Off { velocity: 0 }),
            event(40, 1, Gesture::Off { velocity: 0 }),
            event(50, 3, Gesture::Off { velocity: 0 }),
        ],
        harmony_origin: vec![],
        pattern_origin: vec![],
        follow: None,
        conditions: vec![],
        parameter_locks: vec![
            NoteParameterLock {
                note: note(1),
                parameter: 4,
                value: 0,
            },
            NoteParameterLock {
                note: note(1),
                parameter: 7,
                value: 65_535,
            },
            NoteParameterLock {
                note: note(2),
                parameter: 4,
                value: 41_000,
            },
        ],
    }
}
fn address(parameter: u32) -> ParameterAddress {
    ParameterAddress {
        destination: DestinationId(1),
        parameter,
    }
}
fn engine(clip: Clip, capacity: usize) -> Engine {
    let mut session = Session::default();
    session.slots[0].clip = Some(clip);
    Engine::new(
        API_VERSION,
        session,
        Limits {
            voices: capacity.min(4),
            output_events: capacity,
            recorded_events: 64,
        },
    )
    .unwrap()
}
fn capabilities(e: &mut Engine) {
    e.set_note_parameter_capabilities(&[address(4), address(7)])
        .unwrap();
}
fn launch(e: &mut Engine) {
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(0),
    })
    .unwrap();
}
fn lock(e: &mut Engine, id: u64, parameter: u32, value: Option<u16>) {
    e.command(Command::SetNoteParameter {
        slot: 0,
        note: note(id),
        parameter,
        value,
    })
    .unwrap();
}

#[test]
fn unison_locks_bind_fresh_voices_without_inheriting_to_live_or_unlocked_notes() {
    let mut e = engine(phrase(), 32);
    capabilities(&mut e);
    launch(&mut e);
    e.input(event(0, 99, on())).unwrap();
    e.advance(BeatTime(150)).unwrap();
    let musical: Vec<_> = e.drain_output().collect();
    let parameters: Vec<_> = e.drain_parameter_output().collect();
    let attacks: Vec<_> = musical
        .iter()
        .filter(|e| matches!(e.gesture, Gesture::On { .. }))
        .collect();
    assert_eq!(
        attacks.iter().map(|e| e.at.0).collect::<Vec<_>>(),
        [0, 10, 10, 10, 110, 110, 110]
    );
    assert_eq!(
        parameters
            .iter()
            .map(|e| (e.at.0, e.address.parameter, e.value))
            .collect::<Vec<_>>(),
        [
            (10, 4, 0),
            (10, 7, 65_535),
            (10, 4, 41_000),
            (110, 4, 0),
            (110, 7, 65_535),
            (110, 4, 41_000)
        ]
    );
    assert_eq!(
        parameters
            .iter()
            .map(|e| e.voice.unwrap())
            .collect::<Vec<_>>(),
        [
            attacks[1].voice,
            attacks[1].voice,
            attacks[2].voice,
            attacks[4].voice,
            attacks[4].voice,
            attacks[5].voice
        ]
    );
    assert!(parameters.iter().all(|e| e.voice != Some(attacks[0].voice)
        && e.voice != Some(attacks[3].voice)
        && e.voice != Some(attacks[6].voice)));
}

#[test]
fn capabilities_are_scoped_and_failed_attack_is_retryable_without_partial_publication() {
    let mut clip = phrase();
    clip.events.retain(|e| e.note == note(1));
    clip.parameter_locks.truncate(2);
    let mut e = engine(clip, 3);
    launch(&mut e);
    e.set_parameter_capabilities(&[address(4), address(7)])
        .unwrap();
    assert_eq!(
        e.advance(BeatTime(10)),
        Err(Error::State(
            "destination does not support a note parameter"
        ))
    );
    assert_eq!(e.position(), BeatTime(0));
    assert_eq!(e.snapshot().active_voices, 0);
    assert_eq!(e.drain_output().count(), 0);
    assert_eq!(e.drain_parameter_output().count(), 0);
    capabilities(&mut e);
    e.advance(BeatTime(40)).unwrap();
    assert_eq!(e.drain_output().count(), 3);
    assert_eq!(
        e.advance(BeatTime(110)),
        Err(Error::Capacity("parameter output"))
    );
    assert_eq!(e.snapshot().active_voices, 0);
    assert_eq!(e.drain_output().count(), 0);
    assert_eq!(e.drain_parameter_output().count(), 2);
    e.advance(BeatTime(110)).unwrap();
    let attack = e.drain_output().next().unwrap();
    assert_eq!(attack.voice, VoiceId(2));
    assert!(
        e.drain_parameter_output()
            .all(|event| event.voice == Some(attack.voice))
    );
}

#[test]
fn parameter_pressure_does_not_consume_note_release_capacity() {
    let mut clip = phrase();
    clip.events.retain(|e| e.note == note(1));
    clip.parameter_locks.truncate(2);
    let mut e = engine(clip, 3);
    capabilities(&mut e);
    launch(&mut e);
    e.advance(BeatTime(10)).unwrap();
    e.drain_output().for_each(drop);
    e.input(event(10, 99, on())).unwrap();
    e.drain_output().for_each(drop);
    e.command(Command::Stop).unwrap();
    assert_eq!(
        e.drain_output()
            .filter(|e| matches!(e.gesture, Gesture::Off { .. }))
            .count(),
        2
    );
    assert_eq!(e.drain_parameter_output().count(), 2);
    assert_eq!(e.snapshot().active_voices, 0);
}

#[test]
fn conditions_and_mute_skip_locks_but_preserve_release_and_new_loop_ownership() {
    let clip = phrase()
        .with_note_condition(note(1), NoteCondition::Fill {})
        .unwrap();
    let mut e = engine(clip, 32);
    capabilities(&mut e);
    launch(&mut e);
    e.advance(BeatTime(50)).unwrap();
    e.drain_output().for_each(drop);
    assert_eq!(
        e.drain_parameter_output()
            .map(|e| e.value)
            .collect::<Vec<_>>(),
        [41_000]
    );
    e.command(Command::SetClipMuted {
        slot: 0,
        muted: true,
    })
    .unwrap();
    e.command(Command::SetFill(true)).unwrap();
    e.advance(BeatTime(150)).unwrap();
    assert_eq!(e.drain_output().count(), 0);
    assert_eq!(e.drain_parameter_output().count(), 0);
    e.command(Command::SetClipMuted {
        slot: 0,
        muted: false,
    })
    .unwrap();
    e.advance(BeatTime(210)).unwrap();
    let attacks: Vec<_> = e.drain_output().collect();
    assert_eq!(attacks.len(), 3);
    assert_eq!(
        e.drain_parameter_output()
            .map(|e| e.value)
            .collect::<Vec<_>>(),
        [0, 65_535, 41_000]
    );
    e.command(Command::SetFill(false)).unwrap();
    e.advance(BeatTime(250)).unwrap();
    assert_eq!(
        e.drain_output()
            .filter(|e| matches!(e.gesture, Gesture::Off { .. }))
            .count(),
        3
    );
}

#[test]
fn edits_quantization_duplicate_and_undo_preserve_lock_ownership() {
    let original = phrase();
    let mut e = engine(original.clone(), 32);
    lock(&mut e, 1, 4, Some(222));
    assert_eq!(
        e.session().slots[0].clip.as_ref().unwrap().events,
        original.events
    );
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().slots[0].clip, Some(original.clone()));
    lock(&mut e, 1, 4, None);
    assert_eq!(
        e.session().slots[0]
            .clip
            .as_ref()
            .unwrap()
            .note_parameters(note(1))
            .len(),
        1
    );
    e.command(Command::Undo).unwrap();
    e.command(Command::EditNote {
        slot: 0,
        note: note(1),
        edit: NoteEdit {
            start: BeatTime(15),
            duration: BeatTime(30),
            key: 64,
        },
    })
    .unwrap();
    e.command(Command::QuantizeClip {
        slot: 0,
        quantum: BeatTime(20),
    })
    .unwrap();
    let edited = e.session().slots[0].clip.as_ref().unwrap().clone();
    assert_eq!(edited.parameter_locks, original.parameter_locks);
    e.command(Command::Duplicate { from: 0, to: 1 }).unwrap();
    assert_eq!(e.session().slots[1].clip, Some(edited));
    capabilities(&mut e);
    launch(&mut e);
    assert!(
        e.command(Command::SetNoteParameter {
            slot: 0,
            note: note(1),
            parameter: 4,
            value: None
        })
        .is_err()
    );
}

#[test]
fn old_sessions_migrate_losslessly_and_new_locks_reject_false_older_labels() {
    let mut session = Session {
        version: 9,
        ..Session::default()
    };
    let mut old = phrase();
    old.parameter_locks.clear();
    session.slots[0].clip = Some(old.clone());
    let original = serde_json::to_vec(&session).unwrap();
    let migrated: Session = serde_json::from_slice(&original).unwrap();
    let migrated = migrated.migrate().unwrap();
    assert_eq!(migrated.version, 10);
    assert_eq!(migrated.slots[0].clip, Some(old));
    assert_eq!(serde_json::to_vec(&session).unwrap(), original);
    let mut current = migrated;
    current.slots[0].clip = Some(phrase());
    let encoded = serde_json::to_vec(&current).unwrap();
    let restored: Session = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(restored, current);
    let mut reopened = Engine::new(API_VERSION, restored, Limits::default()).unwrap();
    assert!(reopened.snapshot().note_parameter_capabilities.is_empty());
    assert_eq!(reopened.drain_parameter_output().count(), 0);
    for version in 1..=9 {
        let mut mislabeled = current.clone();
        mislabeled.version = version;
        assert!(mislabeled.migrate().is_err());
    }
}

#[test]
fn invalid_metadata_rejects_without_changing_the_source() {
    let original = phrase();
    for invalid in [
        {
            let mut c = original.clone();
            c.parameter_locks.reverse();
            c
        },
        {
            let mut c = original.clone();
            c.parameter_locks[0].note = note(99);
            c
        },
        {
            let mut c = original.clone();
            c.parameter_locks.insert(0, c.parameter_locks[0]);
            c
        },
        {
            let mut c = original.clone();
            c.parameter_locks = (0..17)
                .map(|parameter| NoteParameterLock {
                    note: note(1),
                    parameter,
                    value: 0,
                })
                .collect();
            c
        },
    ] {
        assert!(invalid.validate(TrackId(1)).is_err());
    }
    assert!(original.with_note_parameter(note(99), 0, Some(1)).is_err());
    let mut capped = original.clone();
    capped.parameter_locks.clear();
    for parameter in 0..16 {
        capped = capped
            .with_note_parameter(note(1), parameter, Some(1))
            .unwrap();
    }
    assert!(capped.with_note_parameter(note(1), 16, Some(1)).is_err());
    assert_eq!(capped.note_parameters(note(1)).len(), 16);
    assert!(
        serde_json::from_str::<NoteParameterLock>(
            r#"{"note":{"source":3,"track":1,"note":1},"parameter":0,"value":65536}"#
        )
        .is_err()
    );
}

#[test]
fn duplicated_route_keeps_locks_and_requires_its_own_capabilities() {
    let mut session = Session::default();
    session.slots[0].clip = Some(phrase());
    session.tracks.push(Track {
        id: TrackId(2),
        name: "Second".into(),
        instrument: InstrumentTarget {
            destination: DestinationId(2),
            reference: "capture://second".into(),
        },
    });
    session.slots[1].track = TrackId(2);
    let mut e = Engine::new(API_VERSION, session, Limits::default()).unwrap();
    capabilities(&mut e);
    e.command(Command::Duplicate { from: 0, to: 1 }).unwrap();
    let duplicate = e.session().slots[1].clip.as_ref().unwrap();
    assert!(
        duplicate
            .parameter_locks
            .iter()
            .all(|entry| entry.note.track == TrackId(2))
    );
    assert_eq!(
        duplicate
            .parameter_locks
            .iter()
            .map(|entry| (entry.parameter, entry.value))
            .collect::<Vec<_>>(),
        [(4, 0), (7, 65_535), (4, 41_000)]
    );
    e.command(Command::Launch {
        slot: 1,
        quantum: BeatTime(0),
    })
    .unwrap();
    assert_eq!(
        e.advance(BeatTime(10)),
        Err(Error::State(
            "destination does not support a note parameter"
        ))
    );
    e.set_note_parameter_capabilities(&[
        address(4),
        address(7),
        ParameterAddress {
            destination: DestinationId(2),
            parameter: 4,
        },
        ParameterAddress {
            destination: DestinationId(2),
            parameter: 7,
        },
    ])
    .unwrap();
    e.advance(BeatTime(50)).unwrap();
    assert!(
        e.drain_parameter_output()
            .all(|event| event.address.destination == DestinationId(2))
    );
    e.drain_output().for_each(drop);
    e.command(Command::Stop).unwrap();
}

#[test]
fn overdub_preserves_original_locks_and_new_notes_keep_defaults() {
    let mut e = engine(phrase(), 32);
    capabilities(&mut e);
    e.advance(BeatTime(50)).unwrap();
    e.command(Command::BeginOverdub { slot: 0 }).unwrap();
    e.advance(BeatTime(51)).unwrap();
    e.input(event(51, 9, on())).unwrap();
    e.advance(BeatTime(52)).unwrap();
    e.input(event(52, 9, Gesture::Off { velocity: 0 })).unwrap();
    e.advance(BeatTime(53)).unwrap();
    e.command(Command::FinishRecord).unwrap();
    let overdub = e.session().slots[0].clip.as_ref().unwrap();
    assert_eq!(overdub.parameter_locks, phrase().parameter_locks);
    assert_eq!(overdub.notes().unwrap().len(), 4);
    e.command(Command::Stop).unwrap();
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().slots[0].clip, Some(phrase()));
}
