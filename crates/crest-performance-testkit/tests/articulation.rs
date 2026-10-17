use crest_performance_core::*;

fn strum(direction: StrumDirection) -> HarmonySettings {
    HarmonySettings {
        articulation: Articulation::Strum {
            direction,
            interval: BeatTime(100),
        },
        ..HarmonySettings::default()
    }
}
fn engine(settings: HarmonySettings, limits: Limits) -> Engine {
    let mut engine = Engine::new(
        API_VERSION,
        Session {
            harmony: settings,
            ..Session::default()
        },
        limits,
    )
    .unwrap();
    engine
        .command(Command::SetMode(SurfaceMode::Chords))
        .unwrap();
    engine
}
fn input(engine: &mut Engine, id: u64, at: u64, gesture: Gesture) -> Result<()> {
    engine.advance(BeatTime(at))?;
    engine.input(InputEvent {
        at: BeatTime(at),
        note: NoteKey {
            source: SourceId(1),
            track: TrackId(1),
            note: NoteId(id),
        },
        gesture,
    })
}
fn on(key: u8) -> Gesture {
    Gesture::On {
        key,
        velocity: 99,
        expression: Expression::default(),
    }
}
fn attacks(events: &[OutputEvent]) -> Vec<(u64, u8, Expression)> {
    events
        .iter()
        .filter_map(|event| match event.gesture {
            Gesture::On {
                key, expression, ..
            } => Some((event.at.0, key, expression)),
            _ => None,
        })
        .collect()
}

#[test]
fn strum_clock_orders_tones_and_uses_latest_expression_without_retuning_held_chords() {
    for (direction, pitches) in [
        (StrumDirection::Ascending, [60, 64, 67]),
        (StrumDirection::Descending, [67, 64, 60]),
    ] {
        let settings = strum(direction);
        let mut e = engine(settings, Limits::default());
        input(&mut e, 1, 0, on(30)).unwrap();
        let changed = Expression {
            pitch: 250_000,
            timbre: 51_000,
            pressure: 43_000,
        };
        input(
            &mut e,
            1,
            50,
            Gesture::Expression {
                expression: changed,
            },
        )
        .unwrap();
        e.command(Command::SetHarmony(HarmonySettings {
            tonic: 62,
            ..settings
        }))
        .unwrap();
        e.advance(BeatTime(200)).unwrap();
        let output: Vec<_> = e.drain_output().collect();
        assert_eq!(
            attacks(&output),
            [
                (0, pitches[0], Expression::default()),
                (100, pitches[1], changed),
                (200, pitches[2], changed)
            ]
        );
        assert_eq!(
            output.len(),
            4,
            "Pending voices must not emit expression before attack"
        );
        assert_eq!(output[0].voice, output[1].voice);
        input(&mut e, 1, 201, Gesture::Off { velocity: 17 }).unwrap();
        let releases: Vec<_> = e.drain_output().collect();
        assert_eq!(releases.len(), 3);
        for attack in output
            .iter()
            .filter(|event| matches!(event.gesture, Gesture::On { .. }))
        {
            assert_eq!(
                releases
                    .iter()
                    .filter(|release| release.voice == attack.voice)
                    .count(),
                1
            );
        }
        assert_eq!(e.snapshot().active_voices, 0);
    }
}

#[test]
fn early_release_stop_disconnect_and_mode_change_cancel_unsounded_tones() {
    for action in 0..4 {
        let mut e = engine(strum(StrumDirection::Ascending), Limits::default());
        input(&mut e, 1, 0, on(30)).unwrap();
        let attack = e.drain_output().next().unwrap();
        e.advance(BeatTime(50)).unwrap();
        match action {
            0 => input(&mut e, 1, 50, Gesture::Off { velocity: 3 }).unwrap(),
            1 => e.command(Command::Stop).unwrap(),
            2 => e.disconnect(SourceId(1)).unwrap(),
            _ => e.command(Command::SetMode(SurfaceMode::Play)).unwrap(),
        }
        let release: Vec<_> = e.drain_output().collect();
        assert_eq!(release.len(), 1);
        assert_eq!(release[0].voice, attack.voice);
        assert!(matches!(release[0].gesture, Gesture::Off { .. }));
        e.advance(BeatTime(1_000)).unwrap();
        assert_eq!(e.drain_output().count(), 0);
        assert_eq!(e.snapshot().active_voices, 0);
    }
}

#[test]
fn three_strums_keep_each_triggers_expression_independent_and_replay_exactly() {
    let mut e = engine(strum(StrumDirection::Ascending), Limits::default());
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    for id in 1..=3 {
        input(&mut e, id, 0, on(30)).unwrap();
    }
    let expressions = [
        Expression {
            pitch: 300_000,
            ..Expression::default()
        },
        Expression {
            timbre: 49_000,
            ..Expression::default()
        },
        Expression {
            pressure: 61_000,
            ..Expression::default()
        },
    ];
    for (index, expression) in expressions.into_iter().enumerate() {
        input(
            &mut e,
            index as u64 + 1,
            50,
            Gesture::Expression { expression },
        )
        .unwrap();
    }
    e.advance(BeatTime(250)).unwrap();
    for id in [3, 1, 2] {
        input(&mut e, id, 250, Gesture::Off { velocity: 0 }).unwrap();
    }
    e.advance(BeatTime(300)).unwrap();
    e.command(Command::FinishRecord).unwrap();
    let original: Vec<_> = e.drain_output().collect();
    let sounded = attacks(&original);
    assert_eq!(sounded.len(), 9);
    for group in sounded[3..].chunks(3) {
        assert_eq!(
            group.iter().map(|item| item.2).collect::<Vec<_>>(),
            expressions
        );
    }
    let saved = e.save_session().unwrap();
    let clip = saved.slots[0].clip.as_ref().unwrap();
    assert_eq!(clip.harmony_origin.len(), 9);
    assert!(
        clip.harmony_origin
            .iter()
            .all(|origin| origin.algorithm_version == HARMONY_ALGORITHM_VERSION)
    );
    let reopened: Session = serde_json::from_str(&serde_json::to_string(&saved).unwrap()).unwrap();
    let mut replay = Engine::new(API_VERSION, reopened, Limits::default()).unwrap();
    input(&mut replay, 90, 0, on(80)).unwrap();
    let live = replay.drain_output().next().unwrap().voice;
    replay
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    replay.advance(BeatTime(299)).unwrap();
    let played: Vec<_> = replay.drain_output().collect();
    assert_eq!(
        original
            .iter()
            .map(|e| (e.at, e.gesture))
            .collect::<Vec<_>>(),
        played.iter().map(|e| (e.at, e.gesture)).collect::<Vec<_>>()
    );
    assert!(played.iter().all(|event| event.voice != live));
}

#[test]
fn pending_capacity_reserves_notes_and_releases_and_output_pressure_is_retryable() {
    let mut e = engine(
        strum(StrumDirection::Ascending),
        Limits {
            voices: 3,
            output_events: 3,
            recorded_events: 6,
        },
    );
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    input(&mut e, 1, 0, on(30)).unwrap();
    assert!(matches!(
        input(&mut e, 2, 0, on(31)),
        Err(Error::Capacity(_))
    ));
    assert!(matches!(
        input(
            &mut e,
            1,
            50,
            Gesture::Expression {
                expression: Expression::default()
            }
        ),
        Err(Error::Capacity(_))
    ));
    e.advance(BeatTime(200)).unwrap();
    assert_eq!(e.drain_output().count(), 3);
    input(&mut e, 1, 201, Gesture::Off { velocity: 0 }).unwrap();
    e.command(Command::FinishRecord).unwrap();
    assert_eq!(e.session().slots[0].clip.as_ref().unwrap().events.len(), 6);

    let mut e = engine(
        strum(StrumDirection::Ascending),
        Limits {
            voices: 3,
            output_events: 3,
            recorded_events: 100,
        },
    );
    input(&mut e, 1, 0, on(30)).unwrap();
    input(
        &mut e,
        1,
        1,
        Gesture::Expression {
            expression: Expression::default(),
        },
    )
    .unwrap();
    input(
        &mut e,
        1,
        2,
        Gesture::Expression {
            expression: Expression::default(),
        },
    )
    .unwrap();
    assert!(matches!(e.advance(BeatTime(200)), Err(Error::Capacity(_))));
    assert_eq!(e.position(), BeatTime(2));
    e.drain_output().for_each(drop);
    e.advance(BeatTime(200)).unwrap();
    assert_eq!(
        attacks(&e.drain_output().collect::<Vec<_>>())
            .iter()
            .map(|e| (e.0, e.1))
            .collect::<Vec<_>>(),
        [(100, 64), (200, 67)]
    );
}

#[test]
fn pending_top_voice_gestures_reserve_their_final_release_metadata() {
    let mut e = engine(
        HarmonySettings {
            expression: ExpressionPolicy::TopVoice,
            ..strum(StrumDirection::Ascending)
        },
        Limits {
            voices: 3,
            output_events: 3,
            recorded_events: 6,
        },
    );
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    input(&mut e, 1, 0, on(30)).unwrap();
    e.drain_output().for_each(drop);
    for at in 1..=4 {
        input(
            &mut e,
            1,
            at,
            Gesture::Expression {
                expression: Expression {
                    pressure: at as u16,
                    ..Expression::default()
                },
            },
        )
        .unwrap();
    }
    assert!(matches!(
        input(
            &mut e,
            1,
            5,
            Gesture::Expression {
                expression: Expression::default()
            }
        ),
        Err(Error::Capacity(_))
    ));
    input(&mut e, 1, 6, Gesture::Off { velocity: 0 }).unwrap();
    e.command(Command::FinishRecord).unwrap();
    let clip = e.session().slots[0].clip.as_ref().unwrap();
    assert_eq!(clip.harmony_origin.len(), 6);
    assert_eq!(clip.events.len(), 2);
    assert_eq!(e.snapshot().active_voices, 0);
}

#[test]
fn strum_rejects_invalid_time_atomically_and_upgrades_old_sessions_losslessly() {
    let mut settings = strum(StrumDirection::Ascending);
    settings.articulation = Articulation::Strum {
        direction: StrumDirection::Ascending,
        interval: BeatTime(0),
    };
    assert!(settings.validate().is_err());
    let mut e = engine(strum(StrumDirection::Ascending), Limits::default());
    assert!(input(&mut e, 1, u64::MAX - 50, on(30)).is_err());
    assert_eq!(e.snapshot().active_voices, 0);
    assert_eq!(e.drain_output().count(), 0);
    let mut old = Session {
        version: 2,
        ..Session::default()
    };
    let raw = serde_json::to_string(&old).unwrap();
    let legacy = raw.replace(",\"articulation\":{\"type\":\"block\"}", "");
    let decoded: Session = serde_json::from_str(&legacy).unwrap();
    let upgraded = decoded.migrate().unwrap();
    assert_eq!(upgraded.version, SESSION_VERSION);
    assert_eq!(upgraded.harmony.articulation, Articulation::Block);
    assert_eq!(upgraded.slots, old.slots);
    old.harmony = strum(StrumDirection::Ascending);
    assert!(old.migrate().is_err());
    assert!(matches!(
        Engine::new(2, Session::default(), Limits::default()),
        Err(Error::Version(2))
    ));
}

#[test]
fn bass_is_immediate_top_voice_policy_is_pitch_based_and_strumplate_is_manual() {
    let mut session = Session {
        harmony: HarmonySettings {
            bass_track: Some(TrackId(2)),
            expression: ExpressionPolicy::TopVoice,
            ..strum(StrumDirection::Descending)
        },
        ..Session::default()
    };
    session.tracks.push(Track {
        id: TrackId(2),
        name: "Bass".into(),
        instrument: InstrumentTarget {
            destination: DestinationId(2),
            reference: "capture://bass".into(),
        },
    });
    let mut e = Engine::new(API_VERSION, session, Limits::default()).unwrap();
    e.command(Command::SetMode(SurfaceMode::Chords)).unwrap();
    input(&mut e, 1, 0, on(30)).unwrap();
    let changed = Expression {
        pressure: 60_000,
        ..Expression::default()
    };
    input(
        &mut e,
        1,
        50,
        Gesture::Expression {
            expression: changed,
        },
    )
    .unwrap();
    e.advance(BeatTime(200)).unwrap();
    let output: Vec<_> = e.drain_output().collect();
    assert_eq!(
        attacks(&output)
            .iter()
            .map(|event| (event.0, event.1))
            .collect::<Vec<_>>(),
        [(0, 67), (0, 48), (100, 64), (200, 60)]
    );
    assert_eq!(output[1].destination, DestinationId(2));
    assert_eq!(output[0].voice, output[2].voice);
    assert_eq!(
        output[2].gesture,
        Gesture::Expression {
            expression: changed
        }
    );
    assert!(
        attacks(&output)
            .iter()
            .all(|event| event.2 == Expression::default())
    );
    e.command(Command::Stop).unwrap();
    assert_eq!(e.drain_output().count(), 4);
    e.command(Command::SetHarmony(HarmonySettings {
        playing: HarmonyPlaying::Strumplate,
        ..strum(StrumDirection::Descending)
    }))
    .unwrap();
    input(&mut e, 2, 300, on(31)).unwrap();
    assert_eq!(
        attacks(&e.drain_output().collect::<Vec<_>>()),
        [(300, 64, Expression::default())]
    );
    e.advance(BeatTime(1000)).unwrap();
    assert_eq!(e.drain_output().count(), 0);
}

#[test]
fn playback_releases_precede_same_tick_strum_attacks_and_checkpoint_omits_unsounded_notes() {
    let mut e = engine(strum(StrumDirection::Ascending), Limits::default());
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    input(&mut e, 1, 0, on(30)).unwrap();
    e.advance(BeatTime(50)).unwrap();
    let checkpoint = e.checkpoint_session_at(BeatTime(75)).unwrap();
    let notes = checkpoint.slots[0].clip.as_ref().unwrap().notes().unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].duration, BeatTime(75));
    assert_eq!(e.snapshot().active_voices, 3);
    input(&mut e, 1, 50, Gesture::Off { velocity: 0 }).unwrap();
    e.advance(BeatTime(100)).unwrap();
    e.command(Command::FinishRecord).unwrap();
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(0),
    })
    .unwrap();
    e.advance(BeatTime(100)).unwrap();
    e.command(Command::SetHarmony(HarmonySettings {
        articulation: Articulation::Strum {
            direction: StrumDirection::Ascending,
            interval: BeatTime(50),
        },
        ..HarmonySettings::default()
    }))
    .unwrap();
    input(&mut e, 2, 100, on(30)).unwrap();
    e.drain_output().for_each(drop);
    e.advance(BeatTime(150)).unwrap();
    let simultaneous: Vec<_> = e.drain_output().collect();
    assert_eq!(simultaneous.len(), 2);
    assert!(matches!(simultaneous[0].gesture, Gesture::Off { .. }));
    assert!(matches!(
        simultaneous[1].gesture,
        Gesture::On { key: 64, .. }
    ));
    assert!(simultaneous.iter().all(|event| event.at == BeatTime(150)));
}

#[test]
fn schema_two_harmony_provenance_and_expression_are_preserved_without_relabeling() {
    let mut e = engine(HarmonySettings::default(), Limits::default());
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    input(&mut e, 1, 7, on(30)).unwrap();
    input(
        &mut e,
        1,
        19,
        Gesture::Expression {
            expression: Expression {
                pitch: -314_159,
                timbre: 49_123,
                pressure: 60_001,
            },
        },
    )
    .unwrap();
    input(&mut e, 1, 31, Gesture::Off { velocity: 37 }).unwrap();
    e.advance(BeatTime(40)).unwrap();
    e.command(Command::FinishRecord).unwrap();
    let mut legacy = e.save_session().unwrap();
    legacy.version = 2;
    for origin in &mut legacy.slots[0].clip.as_mut().unwrap().harmony_origin {
        origin.algorithm_version = 1;
    }
    let json = serde_json::to_string(&legacy)
        .unwrap()
        .replace(",\"articulation\":{\"type\":\"block\"}", "");
    let decoded: Session = serde_json::from_str(&json).unwrap();
    let migrated = decoded.migrate().unwrap();
    assert_eq!(migrated.slots, legacy.slots);
    assert_eq!(migrated.harmony, legacy.harmony);
    assert_eq!(migrated.version, SESSION_VERSION);
    assert_eq!(migrated.clone().migrate().unwrap(), migrated);
    assert!(
        migrated.slots[0]
            .clip
            .as_ref()
            .unwrap()
            .harmony_origin
            .iter()
            .all(|origin| origin.algorithm_version == 1)
    );
}
