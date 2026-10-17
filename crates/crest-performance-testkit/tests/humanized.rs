use crest_performance_core::*;

fn settings() -> HarmonySettings {
    HarmonySettings {
        articulation: Articulation::Humanized {
            max_delay: BeatTime(100),
            velocity_variation: 8,
            seed: 42,
        },
        ..HarmonySettings::default()
    }
}
fn engine(settings: HarmonySettings, limits: Limits) -> Engine {
    let mut e = Engine::new(
        API_VERSION,
        Session {
            harmony: settings,
            ..Session::default()
        },
        limits,
    )
    .unwrap();
    e.command(Command::SetMode(SurfaceMode::Chords)).unwrap();
    e
}
fn note(id: u64) -> NoteKey {
    NoteKey {
        source: SourceId(7),
        track: TrackId(1),
        note: NoteId(id),
    }
}
fn on() -> Gesture {
    Gesture::On {
        key: 30,
        velocity: 90,
        expression: Expression::default(),
    }
}
fn input(e: &mut Engine, at: u64, id: u64, gesture: Gesture) -> Result<()> {
    e.advance(BeatTime(at))?;
    e.input(InputEvent {
        at: BeatTime(at),
        note: note(id),
        gesture,
    })
}
fn attacks(events: &[OutputEvent]) -> Vec<(u64, u8, u8)> {
    events
        .iter()
        .filter_map(|event| match event.gesture {
            Gesture::On { key, velocity, .. } => Some((event.at.0, key, velocity)),
            _ => None,
        })
        .collect()
}

#[test]
fn seeded_chords_match_fixed_timing_and_velocity_vectors_without_reordering_voice_identity() {
    let mut e = engine(settings(), Limits::default());
    input(&mut e, 0, 1, on()).unwrap();
    assert!(e.drain_output().next().is_none());
    input(&mut e, 100, 1, Gesture::Off { velocity: 17 }).unwrap();
    let first: Vec<_> = e.drain_output().collect();
    // Independently calculated SplitMix64 words, seed 42, source 7, track 1.
    assert_eq!(
        attacks(&first),
        vec![(68, 60, 88), (77, 64, 87), (80, 67, 95)]
    );
    assert_eq!(first.len(), 6);
    input(&mut e, 200, 2, on()).unwrap();
    input(&mut e, 300, 2, Gesture::Off { velocity: 18 }).unwrap();
    let second: Vec<_> = e.drain_output().collect();
    assert_eq!(
        attacks(&second),
        vec![(201, 64, 92), (206, 67, 93), (274, 60, 92)]
    );
    assert_eq!(second.len(), 6);
    assert_eq!(e.snapshot().active_voices, 0);
    let mut repeated = engine(settings(), Limits::default());
    input(&mut repeated, 0, 1, on()).unwrap();
    input(&mut repeated, 100, 1, Gesture::Off { velocity: 17 }).unwrap();
    assert_eq!(repeated.drain_output().collect::<Vec<_>>(), first);
}

#[test]
fn expression_before_all_delayed_attacks_and_source_gestures_survive_exact_recording_replay() {
    let mut e = engine(settings(), Limits::default());
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    input(&mut e, 0, 1, on()).unwrap();
    let early = Expression {
        pitch: 250_000,
        pressure: 12000,
        timbre: 45000,
    };
    input(&mut e, 30, 1, Gesture::Expression { expression: early }).unwrap();
    assert_eq!(e.drain_output().count(), 0);
    let later = Expression {
        pitch: -750_000,
        pressure: 34000,
        timbre: 18000,
    };
    input(&mut e, 75, 1, Gesture::Expression { expression: later }).unwrap();
    input(&mut e, 120, 1, Gesture::Off { velocity: 19 }).unwrap();
    e.advance(BeatTime(150)).unwrap();
    e.command(Command::FinishRecord).unwrap();
    let session = e.save_session().unwrap();
    let clip = session.slots[0].clip.as_ref().unwrap();
    let notes = clip.notes().unwrap();
    assert_eq!(notes.len(), 3);
    assert_eq!(notes[0].expression.points[0].value, early);
    assert_eq!(notes[0].expression.points[1].value, later);
    assert_eq!(notes[1].expression.points[0].value, later);
    assert_eq!(notes[2].expression.points[0].value, later);
    assert_eq!(clip.harmony_origin.len(), 4);
    assert_eq!(clip.harmony_origin[0].input.at, BeatTime(0));
    assert!(matches!(
        clip.harmony_origin[0].input.gesture,
        Gesture::On { .. }
    ));
    let reopened: Session = serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
    assert_eq!(reopened, session);
    let mut replay = Engine::new(API_VERSION, reopened, Limits::default()).unwrap();
    replay
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    replay.advance(BeatTime(149)).unwrap();
    assert_eq!(
        replay
            .drain_output()
            .map(|event| (event.at, event.gesture))
            .collect::<Vec<_>>(),
        clip.events
            .iter()
            .map(|event| (event.at, event.gesture))
            .collect::<Vec<_>>()
    );
}

#[test]
fn release_before_or_during_jitter_cancels_only_unplayed_tones_and_modes_cancel_pending_work() {
    let mut e = engine(settings(), Limits::default());
    input(&mut e, 0, 1, on()).unwrap();
    input(&mut e, 67, 1, Gesture::Off { velocity: 0 }).unwrap();
    e.advance(BeatTime(200)).unwrap();
    assert_eq!(e.drain_output().count(), 0);
    input(&mut e, 200, 1, on()).unwrap();
    input(&mut e, 272, 1, Gesture::Off { velocity: 37 }).unwrap();
    e.advance(BeatTime(400)).unwrap();
    let events: Vec<_> = e.drain_output().collect();
    assert_eq!(attacks(&events), vec![(268, 60, 88)]);
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].voice, events[1].voice);
    for command in [Command::Stop, Command::SetMode(SurfaceMode::Play)] {
        let mut e = engine(settings(), Limits::default());
        input(&mut e, 0, 1, on()).unwrap();
        e.command(command).unwrap();
        e.advance(BeatTime(200)).unwrap();
        assert_eq!(e.drain_output().count(), 0);
        assert_eq!(e.snapshot().active_voices, 0);
    }
    let mut e = engine(settings(), Limits::default());
    input(&mut e, 0, 1, on()).unwrap();
    e.disconnect(SourceId(7)).unwrap();
    e.advance(BeatTime(200)).unwrap();
    assert_eq!(e.drain_output().count(), 0);
}

#[test]
fn limited_recording_reserves_all_pending_releases_and_capacity_rejection_is_atomic() {
    let mut e = engine(
        settings(),
        Limits {
            voices: 2,
            ..Limits::default()
        },
    );
    assert!(matches!(input(&mut e, 0, 1, on()), Err(Error::Capacity(_))));
    assert_eq!(e.snapshot().active_voices, 0);
    assert_eq!(e.drain_output().count(), 0);
    let mut e = engine(
        settings(),
        Limits {
            voices: 3,
            output_events: 3,
            recorded_events: 6,
        },
    );
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    input(&mut e, 0, 1, on()).unwrap();
    e.advance(BeatTime(100)).unwrap();
    assert_eq!(e.drain_output().count(), 3);
    assert!(matches!(
        input(
            &mut e,
            100,
            1,
            Gesture::Expression {
                expression: Expression::default()
            }
        ),
        Err(Error::Capacity(_))
    ));
    input(&mut e, 100, 1, Gesture::Off { velocity: 30 }).unwrap();
    assert_eq!(e.drain_output().count(), 3);
    e.command(Command::FinishRecord).unwrap();
    let clip = e.session().slots[0].clip.as_ref().unwrap();
    assert_eq!(clip.events.len(), 6);
    assert_eq!(clip.harmony_origin.len(), 2);
}

#[test]
fn manual_strumplate_and_separate_bass_remain_immediate_and_top_voice_policy_is_pitch_based() {
    let mut s = settings();
    s.playing = HarmonyPlaying::Strumplate;
    let mut e = engine(s, Limits::default());
    input(&mut e, 0, 1, on()).unwrap();
    assert_eq!(
        attacks(&e.drain_output().collect::<Vec<_>>()),
        vec![(0, 60, 90)]
    );
    let mut s = settings();
    s.bass_track = Some(TrackId(2));
    s.expression = ExpressionPolicy::TopVoice;
    let mut session = Session {
        harmony: s,
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
    input(&mut e, 0, 2, on()).unwrap();
    let expressive = Expression {
        pitch: 750_000,
        pressure: 50000,
        timbre: 5000,
    };
    input(
        &mut e,
        0,
        2,
        Gesture::Expression {
            expression: expressive,
        },
    )
    .unwrap();
    e.advance(BeatTime(100)).unwrap();
    let events: Vec<_> = e.drain_output().collect();
    assert_eq!(
        attacks(&events),
        vec![(0, 48, 90), (1, 64, 92), (6, 67, 93), (74, 60, 92)]
    );
    for event in events {
        if let Gesture::On {
            key, expression, ..
        } = event.gesture
        {
            assert_eq!(
                expression,
                if key == 67 {
                    expressive
                } else {
                    Expression::default()
                }
            );
        }
    }
}

#[test]
fn humanization_bounds_and_versioned_provenance_are_enforced() {
    for articulation in [
        Articulation::Humanized {
            max_delay: BeatTime(TICKS_PER_BEAT + 1),
            velocity_variation: 8,
            seed: 0,
        },
        Articulation::Humanized {
            max_delay: BeatTime(0),
            velocity_variation: 64,
            seed: 0,
        },
    ] {
        assert!(articulation.validate().is_err());
    }
    let mut s = settings();
    s.articulation = Articulation::Humanized {
        max_delay: BeatTime(0),
        velocity_variation: 0,
        seed: 0,
    };
    let mut e = engine(s, Limits::default());
    input(&mut e, 0, 1, on()).unwrap();
    assert_eq!(
        attacks(&e.drain_output().collect::<Vec<_>>()),
        vec![(0, 60, 90), (0, 64, 90), (0, 67, 90)]
    );
    for version in 1..=6 {
        let false_label = Session {
            version,
            harmony: settings(),
            ..Session::default()
        };
        assert!(false_label.migrate().is_err());
        let old = Session {
            version,
            ..Session::default()
        };
        assert_eq!(old.migrate().unwrap().version, SESSION_VERSION);
    }
    for version in 1..=4 {
        assert!(
            HarmonyGesture {
                algorithm_version: version,
                input: InputEvent {
                    at: BeatTime(0),
                    note: note(1),
                    gesture: on()
                },
                settings: settings()
            }
            .validate()
            .is_err()
        );
    }
    for version in 1..=3 {
        assert!(
            PatternOrigin {
                algorithm_version: version,
                settings: PatternSettings::default(),
                harmony: settings()
            }
            .validate()
            .is_err()
        );
    }
}

#[test]
fn three_simultaneous_humanized_chords_keep_xyz_separate_across_pending_and_sounding_voices() {
    let mut e = engine(settings(), Limits::default());
    for id in 1..=3 {
        input(&mut e, 0, id, on()).unwrap();
    }
    let expressions = [
        Expression {
            pitch: 750_000,
            ..Expression::default()
        },
        Expression {
            timbre: 60000,
            ..Expression::default()
        },
        Expression {
            pressure: 50000,
            ..Expression::default()
        },
    ];
    for (index, expression) in expressions.into_iter().enumerate() {
        input(
            &mut e,
            30,
            index as u64 + 1,
            Gesture::Expression { expression },
        )
        .unwrap();
    }
    e.command(Command::SetHarmony(HarmonySettings {
        tonic: 72,
        ..HarmonySettings::default()
    }))
    .unwrap();
    e.advance(BeatTime(100)).unwrap();
    let mut latest = std::collections::BTreeMap::new();
    let mut pitches = Vec::new();
    for event in e.drain_output() {
        if let Gesture::On {
            key, expression, ..
        } = event.gesture
        {
            pitches.push(key);
            latest.insert(event.voice, expression);
        } else if let Gesture::Expression { expression } = event.gesture {
            latest.insert(event.voice, expression);
        }
    }
    pitches.sort();
    assert_eq!(pitches, [60, 60, 60, 64, 64, 64, 67, 67, 67]);
    assert_eq!(latest.len(), 9);
    for (voice, expression) in latest {
        assert_eq!(expression, expressions[(voice.0 as usize - 1) / 3]);
    }
    for id in 1..=3 {
        input(&mut e, 200, id, Gesture::Off { velocity: 0 }).unwrap();
    }
    assert_eq!(e.drain_output().count(), 9);
    assert_eq!(e.snapshot().active_voices, 0);
}

#[test]
fn old_schema_cannot_hide_humanization_in_followers_or_historical_provenance() {
    for version in 1..=6 {
        let mut session = Session {
            version,
            ..Session::default()
        };
        let mut clip = PatternSettings::default()
            .render(HarmonySettings::default(), TrackId(1))
            .unwrap();
        // A valid historical generator label isolates the nested follower check.
        clip.pattern_origin[0].algorithm_version = 1;
        clip.follow = Some(HarmonyFollow {
            reference: settings(),
        });
        session.slots[0].clip = Some(clip.clone());
        assert!(session.clone().migrate().is_err());
        clip.follow = None;
        clip.pattern_origin[0].harmony = settings();
        clip.pattern_origin[0].algorithm_version = PATTERN_ALGORITHM_VERSION;
        session.slots[0].clip = Some(clip.clone());
        assert!(session.clone().migrate().is_err());
        clip.pattern_origin.clear();
        clip.harmony_origin.push(HarmonyGesture {
            algorithm_version: HARMONY_ALGORITHM_VERSION,
            input: InputEvent {
                at: BeatTime(0),
                note: note(1),
                gesture: on(),
            },
            settings: settings(),
        });
        session.slots[0].clip = Some(clip);
        assert!(session.migrate().is_err());
    }
}
