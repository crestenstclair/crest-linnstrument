use crest_performance_core::*;

fn engine(limits: Limits) -> Engine {
    let mut e = Engine::new(API_VERSION, Session::default(), limits).unwrap();
    e.command(Command::SetMode(SurfaceMode::Chords)).unwrap();
    e
}
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
fn pitches(events: &[OutputEvent]) -> Vec<u8> {
    events
        .iter()
        .filter_map(|e| {
            if let Gesture::On { key, .. } = e.gesture {
                Some(key)
            } else {
                None
            }
        })
        .collect()
}
#[test]
fn diatonic_degrees_extensions_borrowing_and_voicing_have_independent_expectations() {
    let defaults = HarmonySettings::default();
    let expected = [
        [60, 64, 67],
        [62, 65, 69],
        [64, 67, 71],
        [65, 69, 72],
        [67, 71, 74],
        [69, 72, 76],
        [71, 74, 77],
    ];
    for (degree, notes) in expected.into_iter().enumerate() {
        assert_eq!(defaults.voicing(degree as u8, None).unwrap().notes(), notes);
    }
    let seventh = HarmonySettings {
        extension: ChordExtension::Seventh,
        ..defaults
    };
    assert_eq!(seventh.voicing(4, None).unwrap().notes(), [67, 71, 74, 77]);
    assert_eq!(
        HarmonySettings {
            extension: ChordExtension::Ninth,
            ..defaults
        }
        .voicing(0, None)
        .unwrap()
        .notes(),
        [60, 64, 67, 71, 74]
    );
    assert_eq!(
        HarmonySettings {
            suspension: Suspension::Fourth,
            ..defaults
        }
        .voicing(0, None)
        .unwrap()
        .notes(),
        [60, 65, 67]
    );
    assert_eq!(
        HarmonySettings {
            borrowed: true,
            ..defaults
        }
        .voicing(3, None)
        .unwrap()
        .notes(),
        [65, 68, 72]
    );
    assert_eq!(
        HarmonySettings {
            inversion: 1,
            ..defaults
        }
        .voicing(0, None)
        .unwrap()
        .notes(),
        [64, 67, 72]
    );
    assert_eq!(
        HarmonySettings {
            inversion: -1,
            ..defaults
        }
        .voicing(0, None)
        .unwrap()
        .notes(),
        [55, 60, 64]
    );
    assert_eq!(
        HarmonySettings {
            spread: true,
            ..defaults
        }
        .voicing(0, None)
        .unwrap()
        .notes(),
        [60, 67, 76]
    );
    assert_eq!(
        HarmonySettings {
            register: -1,
            ..defaults
        }
        .voicing(0, None)
        .unwrap()
        .notes(),
        [48, 52, 55]
    );
    let c = defaults.voicing(0, None).unwrap();
    assert_eq!(
        HarmonySettings {
            voice_leading: true,
            ..defaults
        }
        .voicing(4, Some(&c))
        .unwrap()
        .notes(),
        [59, 62, 67]
    );
    assert!(
        HarmonySettings {
            tonic: 126,
            ..defaults
        }
        .validate()
        .is_err()
    );
    assert!(defaults.voicing(7, None).is_err());
}
#[test]
fn three_chord_gestures_own_independent_expression_and_release_every_voice() {
    let mut e = engine(Limits::default());
    for id in 1..=3 {
        e.input(event(id, 0, on(29 + id as u8))).unwrap();
    }
    let attacks: Vec<_> = e.drain_output().collect();
    assert_eq!(pitches(&attacks), [60, 64, 67, 62, 65, 69, 64, 67, 71]);
    let expressions = [
        Expression {
            pitch: 250_000,
            ..Expression::default()
        },
        Expression {
            timbre: 54_000,
            ..Expression::default()
        },
        Expression {
            pressure: 45_000,
            ..Expression::default()
        },
    ];
    e.advance(BeatTime(101)).unwrap();
    for (index, expression) in expressions.into_iter().enumerate() {
        e.input(event(
            index as u64 + 1,
            101,
            Gesture::Expression { expression },
        ))
        .unwrap();
        let batch: Vec<_> = e.drain_output().collect();
        assert_eq!(batch.len(), 3);
        for (voice, attack) in batch.iter().zip(&attacks[index * 3..index * 3 + 3]) {
            assert_eq!(voice.voice, attack.voice);
            assert_eq!(voice.gesture, Gesture::Expression { expression });
        }
    }
    // Changing the scale never emits a retune or changes existing ownership.
    e.command(Command::SetHarmony(HarmonySettings {
        tonic: 63,
        scale: Scale::NaturalMinor,
        ..HarmonySettings::default()
    }))
    .unwrap();
    assert_eq!(e.drain_output().count(), 0);
    for id in [3, 1, 2] {
        e.input(event(id, 101, Gesture::Off { velocity: 17 }))
            .unwrap();
    }
    let releases: Vec<_> = e.drain_output().collect();
    assert_eq!(releases.len(), 9);
    for attack in attacks {
        assert_eq!(
            releases.iter().filter(|r| r.voice == attack.voice).count(),
            1
        );
    }
    assert_eq!(e.snapshot().active_voices, 0);
}
#[test]
fn top_voice_policy_is_fixed_at_attack_and_bass_has_an_independent_destination() {
    let mut session = Session::default();
    session.tracks.push(Track {
        id: TrackId(2),
        name: "Bass".into(),
        instrument: InstrumentTarget {
            destination: DestinationId(2),
            reference: "capture://bass".into(),
        },
    });
    session.harmony.expression = ExpressionPolicy::TopVoice;
    session.harmony.bass_track = Some(TrackId(2));
    let mut e = Engine::new(API_VERSION, session, Limits::default()).unwrap();
    e.command(Command::SetMode(SurfaceMode::Chords)).unwrap();
    e.input(event(1, 0, on(30))).unwrap();
    let attacks: Vec<_> = e.drain_output().collect();
    assert_eq!(pitches(&attacks), [60, 64, 67, 48]);
    assert_eq!(attacks[3].destination, DestinationId(2));
    e.command(Command::SetHarmony(HarmonySettings::default()))
        .unwrap();
    e.input(event(
        1,
        0,
        Gesture::Expression {
            expression: Expression {
                pressure: 61_000,
                ..Expression::default()
            },
        },
    ))
    .unwrap();
    let changed: Vec<_> = e.drain_output().collect();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0].voice, attacks[2].voice);
    e.disconnect(SourceId(1)).unwrap();
    assert_eq!(e.drain_output().count(), 4);
    assert_eq!(e.snapshot().active_voices, 0);
}
#[test]
fn chord_capacity_and_expression_are_atomic_and_recording_reserves_all_releases() {
    let mut e = engine(Limits {
        voices: 3,
        output_events: 4,
        recorded_events: 6,
    });
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    e.input(event(1, 0, on(30))).unwrap();
    assert!(matches!(
        e.input(event(2, 0, on(31))),
        Err(Error::Capacity(_))
    ));
    assert_eq!(e.snapshot().active_voices, 3);
    assert_eq!(e.drain_output().count(), 3);
    assert!(matches!(
        e.input(event(
            1,
            0,
            Gesture::Expression {
                expression: Expression::default()
            }
        )),
        Err(Error::Capacity(_))
    ));
    assert_eq!(e.drain_output().count(), 0);
    e.advance(BeatTime(10)).unwrap();
    e.input(event(1, 10, Gesture::Off { velocity: 0 })).unwrap();
    assert_eq!(e.drain_output().count(), 3);
    e.command(Command::FinishRecord).unwrap();
    assert_eq!(e.session().slots[0].clip.as_ref().unwrap().events.len(), 6);
    e.session().validate().unwrap();
    // Output pressure also rejects the entire fan-out, preserving retry.
    let mut e = engine(Limits {
        voices: 3,
        output_events: 3,
        recorded_events: 20,
    });
    e.input(event(1, 0, on(30))).unwrap();
    assert!(e.input(event(1, 0, Gesture::Off { velocity: 0 })).is_err());
    assert_eq!(e.snapshot().active_voices, 3);
    e.drain_output().for_each(drop);
    e.input(event(1, 0, Gesture::Off { velocity: 0 })).unwrap();
    assert_eq!(e.snapshot().active_voices, 0);
}
#[test]
fn rendered_chord_and_origin_survive_reopen_edit_undo_and_live_playback() {
    let mut e = engine(Limits::default());
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    e.input(event(1, 0, on(30))).unwrap();
    e.advance(BeatTime(1234)).unwrap();
    e.input(event(
        1,
        1234,
        Gesture::Expression {
            expression: Expression {
                pitch: 400_000,
                timbre: 51_000,
                pressure: 43_000,
            },
        },
    ))
    .unwrap();
    e.advance(BeatTime(4567)).unwrap();
    e.input(event(1, 4567, Gesture::Off { velocity: 2 }))
        .unwrap();
    e.advance(BeatTime(10_000)).unwrap();
    e.command(Command::FinishRecord).unwrap();
    let original: Vec<_> = e.drain_output().collect();
    let saved = e.save_session().unwrap();
    let clip = saved.slots[0].clip.as_ref().unwrap();
    assert_eq!(clip.notes().unwrap().len(), 3);
    assert_eq!(clip.harmony_origin.len(), 3);
    assert_eq!(clip.harmony_origin[0].input.gesture, on(30));
    let decoded: Session = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(saved, decoded);
    let mut replay = Engine::new(API_VERSION, decoded, Limits::default()).unwrap();
    replay
        .command(Command::SetMode(SurfaceMode::Chords))
        .unwrap();
    replay.input(event(90, 0, on(31))).unwrap();
    let live: Vec<_> = replay.drain_output().collect();
    replay
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    replay.advance(BeatTime(9_999)).unwrap();
    let played: Vec<_> = replay.drain_output().collect();
    assert_eq!(played.len(), original.len());
    for (a, b) in original.iter().zip(&played) {
        assert_eq!((a.at, a.gesture), (b.at, b.gesture));
        assert!(!live.iter().any(|l| l.voice == b.voice));
    }
    replay.command(Command::SetMode(SurfaceMode::Play)).unwrap();
    assert_eq!(replay.drain_output().count(), 3);
    assert_eq!(replay.snapshot().slots[0], SlotState::Playing);
    replay.command(Command::StopSlot { slot: 0 }).unwrap();
    replay
        .command(Command::EditNote {
            slot: 0,
            note: clip.notes().unwrap()[0].id,
            edit: NoteEdit {
                start: BeatTime(3),
                duration: BeatTime(5000),
                key: 61,
            },
        })
        .unwrap();
    assert_eq!(
        replay.session().slots[0]
            .clip
            .as_ref()
            .unwrap()
            .harmony_origin,
        clip.harmony_origin
    );
    replay.command(Command::Undo).unwrap();
    assert_eq!(replay.session().slots[0].clip.as_ref(), Some(clip));
}
#[test]
fn strumplate_picks_chord_tones_and_keeps_held_note_after_degree_change() {
    let mut e = engine(Limits::default());
    let settings = HarmonySettings {
        playing: HarmonyPlaying::Strumplate,
        ..HarmonySettings::default()
    };
    e.command(Command::SetHarmony(settings)).unwrap();
    for (id, key) in [(1, 29), (2, 30), (3, 31), (4, 32), (5, 33)] {
        e.input(event(id, 0, on(key))).unwrap();
    }
    let notes: Vec<_> = e.drain_output().collect();
    assert_eq!(pitches(&notes), [55, 60, 64, 67, 72]);
    e.command(Command::SetHarmony(HarmonySettings {
        degree: 1,
        ..settings
    }))
    .unwrap();
    e.input(event(6, 0, on(30))).unwrap();
    assert_eq!(pitches(&e.drain_output().collect::<Vec<_>>()), [62]);
    e.input(event(2, 0, Gesture::Off { velocity: 0 })).unwrap();
    assert_eq!(e.drain_output().next().unwrap().voice, notes[1].voice);
    e.command(Command::Stop).unwrap();
    assert_eq!(e.drain_output().count(), 5);
}
#[test]
fn schema_v1_migrates_without_changing_raw_events_and_unknown_versions_fail_closed() {
    let raw = include_str!("../../../examples/expressive-phrase.json");
    let old: Session = serde_json::from_str(raw).unwrap();
    assert_eq!(old.version, 1);
    let upgraded = old.clone().migrate().unwrap();
    assert_eq!(upgraded.version, SESSION_VERSION);
    assert_eq!(upgraded.slots, old.slots);
    assert_eq!(upgraded.harmony, HarmonySettings::default());
    assert_eq!(upgraded.clone().migrate().unwrap(), upgraded);
    assert!(
        Session {
            version: 999,
            ..upgraded
        }
        .migrate()
        .is_err()
    );
    let mut spoof = old;
    spoof.harmony.tonic = 61;
    assert!(spoof.migrate().is_err());
}
