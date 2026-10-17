use crest_performance_core::*;

fn note(id: u64) -> NoteKey {
    NoteKey {
        source: SourceId(7),
        track: TrackId(1),
        note: NoteId(id),
    }
}
fn phrase() -> Clip {
    let mut events = Vec::new();
    for id in 1..=3 {
        events.push(InputEvent {
            at: BeatTime(0),
            note: note(id),
            gesture: Gesture::On {
                key: 60,
                velocity: 80 + id as u8,
                expression: Expression::default(),
            },
        });
    }
    for id in 1..=3 {
        events.push(InputEvent {
            at: BeatTime(20),
            note: note(id),
            gesture: Gesture::Expression {
                expression: Expression {
                    pitch: id as i32 * 1_000_000,
                    pressure: id as u16 * 10_000,
                    timbre: id as u16 * 12_000,
                },
            },
        });
    }
    for id in 1..=3 {
        events.push(InputEvent {
            at: BeatTime(40),
            note: note(id),
            gesture: Gesture::Off { velocity: id as u8 },
        });
    }
    Clip {
        length: BeatTime(100),
        events,
        harmony_origin: vec![],
        pattern_origin: vec![],
        follow: None,
        conditions: vec![
            ConditionalNote {
                note: note(1),
                condition: NoteCondition::Every {
                    period: 4,
                    offset: 3,
                },
            },
            ConditionalNote {
                note: note(2),
                condition: NoteCondition::Fill {},
            },
            ConditionalNote {
                note: note(3),
                condition: NoteCondition::NotFill {},
            },
        ],
    }
}
fn engine(clip: Clip, limits: Limits) -> Engine {
    let mut session = Session::default();
    session.slots[0].clip = Some(clip);
    Engine::new(API_VERSION, session, limits).unwrap()
}
fn launch(e: &mut Engine) {
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(0),
    })
    .unwrap();
}
fn drain_to(e: &mut Engine, ticks: u64) -> Vec<OutputEvent> {
    e.advance(BeatTime(ticks)).unwrap();
    e.drain_output().collect()
}
fn attacks(events: &[OutputEvent]) -> Vec<(u64, u8)> {
    events
        .iter()
        .filter_map(|event| match event.gesture {
            Gesture::On { velocity, .. } => Some((event.at.0, velocity)),
            _ => None,
        })
        .collect()
}

#[test]
fn fill_is_sampled_at_attack_and_repeated_pitches_keep_independent_curves() {
    let mut e = engine(phrase(), Limits::default());
    launch(&mut e);
    let mut events = drain_to(&mut e, 10);
    e.command(Command::SetFill(true)).unwrap();
    events.extend(drain_to(&mut e, 110));
    e.command(Command::SetFill(false)).unwrap();
    events.extend(drain_to(&mut e, 299));
    assert_eq!(attacks(&events), vec![(0, 83), (100, 82), (200, 83)]);
    assert_eq!(events.len(), 9);
    for group in events.chunks_exact(3) {
        let Gesture::On { velocity, .. } = group[0].gesture else {
            panic!()
        };
        let id = velocity - 80;
        assert_eq!(group[1].voice, group[0].voice);
        assert_eq!(group[2].voice, group[0].voice);
        assert!(
            matches!(group[1].gesture, Gesture::Expression {expression} if expression.pitch == i32::from(id)*1_000_000 && expression.pressure == u16::from(id)*10_000 && expression.timbre == u16::from(id)*12_000)
        );
        assert_eq!(group[2].gesture, Gesture::Off { velocity: id });
        assert_eq!(group[2].at.0 - group[0].at.0, 40);
    }
    assert_eq!(e.snapshot().active_voices, 0);
}

#[test]
fn fourth_loop_rule_restarts_on_launch_and_does_not_consume_voices_when_skipped() {
    let mut clip = phrase();
    clip.events.retain(|event| event.note == note(1));
    clip.conditions.truncate(1);
    let mut e = engine(
        clip,
        Limits {
            voices: 1,
            output_events: 1,
            ..Limits::default()
        },
    );
    launch(&mut e);
    let mut events = Vec::new();
    loop {
        let result = e.advance(BeatTime(999));
        events.extend(e.drain_output());
        match result {
            Ok(()) => break,
            Err(Error::Capacity(_)) => {}
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(attacks(&events), vec![(300, 81), (700, 81)]);
    assert_eq!(events.len(), 6);
    assert_eq!(e.snapshot().active_voices, 0);
    launch(&mut e);
    assert!(drain_to(&mut e, 1298).is_empty());
    assert_eq!(attacks(&drain_to(&mut e, 1299)), vec![(1299, 81)]);
    e.command(Command::Stop).unwrap();
    assert_eq!(e.drain_output().count(), 1);
}

#[test]
fn queued_launch_fill_and_live_mpe_do_not_share_ownership() {
    let mut e = engine(phrase(), Limits::default());
    e.advance(BeatTime(1)).unwrap();
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(100),
    })
    .unwrap();
    e.command(Command::SetFill(true)).unwrap();
    let live = InputEvent {
        at: BeatTime(1),
        note: note(99),
        gesture: Gesture::On {
            key: 60,
            velocity: 99,
            expression: Expression::default(),
        },
    };
    e.input(live).unwrap();
    let events = drain_to(&mut e, 150);
    assert_eq!(attacks(&events), vec![(1, 99), (100, 82)]);
    assert_eq!(e.snapshot().active_voices, 1);
    assert_ne!(events[0].voice, events[1].voice);
    e.input(InputEvent {
        at: BeatTime(150),
        gesture: Gesture::Off { velocity: 7 },
        ..live
    })
    .unwrap();
    assert_eq!(e.drain_output().next().unwrap().voice, events[0].voice);
    e.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
    assert!(!e.snapshot().fill);
    e.command(Command::SetFill(true)).unwrap();
    e.disconnect(SourceId(7)).unwrap();
    assert!(!e.snapshot().fill);
    e.command(Command::SetFill(true)).unwrap();
    e.command(Command::Stop).unwrap();
    assert!(!e.snapshot().fill);
    assert!(drain_to(&mut e, 1000).is_empty());
}

#[test]
fn edits_undo_duplicate_and_overdub_preserve_rules_and_original_gestures() {
    let original = phrase();
    let mut e = engine(original.clone(), Limits::default());
    e.command(Command::SetNoteCondition {
        slot: 0,
        note: note(1),
        condition: NoteCondition::Always {},
    })
    .unwrap();
    assert_eq!(
        e.session().slots[0].clip.as_ref().unwrap().events,
        original.events
    );
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().slots[0].clip, Some(original.clone()));
    e.command(Command::EditNote {
        slot: 0,
        note: note(2),
        edit: NoteEdit {
            key: 72,
            start: BeatTime(10),
            duration: BeatTime(40),
        },
    })
    .unwrap();
    assert_eq!(
        e.session().slots[0].clip.as_ref().unwrap().conditions,
        original.conditions
    );
    e.command(Command::Undo).unwrap();
    e.command(Command::Duplicate { from: 0, to: 1 }).unwrap();
    assert_eq!(e.session().slots[1].clip, Some(original.clone()));
    e.command(Command::BeginOverdub { slot: 0 }).unwrap();
    e.input(InputEvent {
        at: BeatTime(0),
        note: note(9),
        gesture: Gesture::On {
            key: 72,
            velocity: 100,
            expression: Expression::default(),
        },
    })
    .unwrap();
    drain_to(&mut e, 50);
    e.input(InputEvent {
        at: BeatTime(50),
        note: note(9),
        gesture: Gesture::Off { velocity: 0 },
    })
    .unwrap();
    e.command(Command::FinishRecord).unwrap();
    let result = e.session().slots[0].clip.as_ref().unwrap();
    assert_eq!(result.conditions, original.conditions);
    assert_eq!(result.events.len(), 11);
    result.validate(TrackId(1)).unwrap();
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().slots[0].clip, Some(original));
}

#[test]
fn duplication_remaps_conditions_to_destination_track() {
    let mut session = Session::default();
    session.slots[0].clip = Some(phrase());
    session.tracks.push(Track {
        id: TrackId(2),
        name: "Bass".into(),
        instrument: InstrumentTarget {
            destination: DestinationId(2),
            reference: "capture://bass".into(),
        },
    });
    session.slots[1].track = TrackId(2);
    let mut e = Engine::new(API_VERSION, session, Limits::default()).unwrap();
    e.command(Command::Duplicate { from: 0, to: 1 }).unwrap();
    let clip = e.session().slots[1].clip.as_ref().unwrap();
    clip.validate(TrackId(2)).unwrap();
    assert!(
        clip.conditions
            .iter()
            .all(|entry| entry.note.track == TrackId(2))
    );
    e.command(Command::Launch {
        slot: 1,
        quantum: BeatTime(0),
    })
    .unwrap();
    assert!(
        drain_to(&mut e, 99)
            .iter()
            .all(|event| event.destination == DestinationId(2))
    );
}

#[test]
fn invalid_or_active_edits_leave_clip_and_undo_unchanged() {
    let original = phrase();
    let mut e = engine(original.clone(), Limits::default());
    e.command(Command::SetNoteCondition {
        slot: 0,
        note: note(1),
        condition: NoteCondition::Always {},
    })
    .unwrap();
    let changed = e.save_session().unwrap();
    for condition in [
        NoteCondition::Every {
            period: 0,
            offset: 0,
        },
        NoteCondition::Every {
            period: 4,
            offset: 4,
        },
    ] {
        assert!(
            e.command(Command::SetNoteCondition {
                slot: 0,
                note: note(1),
                condition
            })
            .is_err()
        );
        assert_eq!(e.save_session().unwrap(), changed);
    }
    assert!(
        e.command(Command::SetNoteCondition {
            slot: 0,
            note: note(999),
            condition: NoteCondition::Fill {}
        })
        .is_err()
    );
    launch(&mut e);
    assert!(
        e.command(Command::SetNoteCondition {
            slot: 0,
            note: note(1),
            condition: NoteCondition::Fill {}
        })
        .is_err()
    );
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().slots[0].clip, Some(original));
}

#[test]
fn persistence_roundtrip_is_exact_fill_is_ephemeral_and_old_labels_cannot_hide_rules() {
    let mut e = engine(phrase(), Limits::default());
    e.command(Command::SetFill(true)).unwrap();
    let saved = e.save_session().unwrap();
    let json = serde_json::to_vec(&saved).unwrap();
    let reopened: Session = serde_json::from_slice(&json).unwrap();
    assert_eq!(reopened, saved);
    let mut replay = Engine::new(API_VERSION, reopened, Limits::default()).unwrap();
    assert!(!replay.snapshot().fill);
    launch(&mut replay);
    assert_eq!(attacks(&drain_to(&mut replay, 99)), vec![(0, 83)]);
    for version in 1..=5 {
        let mut false_label = saved.clone();
        false_label.version = version;
        assert!(false_label.migrate().is_err());
        let mut old = Session {
            version,
            ..Session::default()
        };
        let mut clip = phrase();
        clip.conditions.clear();
        old.slots[0].clip = Some(clip.clone());
        let migrated = old.migrate().unwrap();
        assert_eq!(migrated.version, 6);
        assert_eq!(migrated.slots[0].clip, Some(clip));
    }
}

#[test]
fn untrusted_conditions_reject_bad_values_duplicate_missing_and_reused_notes() {
    let original = phrase();
    for entry in [
        ConditionalNote {
            note: note(1),
            condition: NoteCondition::Always {},
        },
        ConditionalNote {
            note: note(1),
            condition: NoteCondition::Every {
                period: 0,
                offset: 0,
            },
        },
        ConditionalNote {
            note: note(99),
            condition: NoteCondition::Fill {},
        },
    ] {
        let mut clip = original.clone();
        clip.conditions = vec![entry];
        assert!(clip.validate(TrackId(1)).is_err());
    }
    let mut duplicate = original.clone();
    duplicate.conditions.push(duplicate.conditions[0]);
    assert!(duplicate.validate(TrackId(1)).is_err());
    let mut unordered = original.clone();
    unordered.conditions.swap(0, 2);
    assert!(unordered.validate(TrackId(1)).is_err());
    let mut reused = original.clone();
    reused.events.push(InputEvent {
        at: BeatTime(50),
        ..original.events[0]
    });
    reused.events.push(InputEvent {
        at: BeatTime(90),
        ..original.events[6]
    });
    assert!(reused.validate(TrackId(1)).is_err());
    assert!(serde_json::from_str::<NoteCondition>(r#"{"type":"fill","unexpected":1}"#).is_err());
}

#[test]
fn fill_release_at_loop_end_precedes_skipped_next_attack() {
    let mut clip = phrase();
    clip.events.retain(|event| event.note == note(2));
    clip.events.last_mut().unwrap().at = clip.length;
    clip.conditions.retain(|entry| entry.note == note(2));
    let mut e = engine(clip, Limits::default());
    e.command(Command::SetFill(true)).unwrap();
    launch(&mut e);
    let first = drain_to(&mut e, 99);
    e.command(Command::SetFill(false)).unwrap();
    let boundary = drain_to(&mut e, 199);
    assert_eq!(boundary.len(), 1);
    assert_eq!(boundary[0].voice, first[0].voice);
    assert_eq!(boundary[0].at, BeatTime(100));
    assert!(matches!(boundary[0].gesture, Gesture::Off { .. }));
    assert_eq!(e.snapshot().active_voices, 0);
}

#[test]
fn each_clip_counts_its_own_launch_iterations() {
    let mut clip = phrase();
    clip.events.retain(|event| event.note == note(1));
    clip.conditions.truncate(1);
    let mut e = engine(clip, Limits::default());
    e.command(Command::Duplicate { from: 0, to: 1 }).unwrap();
    launch(&mut e);
    assert!(drain_to(&mut e, 150).is_empty());
    e.command(Command::Launch {
        slot: 1,
        quantum: BeatTime(0),
    })
    .unwrap();
    let events = drain_to(&mut e, 699);
    assert_eq!(attacks(&events), vec![(300, 81), (450, 81)]);
    assert_eq!(events.len(), 6);
    assert_ne!(events[0].voice, events[3].voice);
}
