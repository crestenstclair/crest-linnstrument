use crest_performance_core::*;

fn engine() -> Engine {
    Engine::new(API_VERSION, Session::default(), Limits::default()).unwrap()
}
fn input(e: &mut Engine, source: u32, id: u64, at: u64, gesture: Gesture) {
    e.advance(BeatTime(at)).unwrap();
    e.input(InputEvent {
        at: BeatTime(at),
        note: NoteKey {
            source: SourceId(source),
            track: TrackId(1),
            note: NoteId(id),
        },
        gesture,
    })
    .unwrap();
}
fn on(key: u8) -> Gesture {
    Gesture::On {
        key,
        velocity: 99,
        expression: Expression::default(),
    }
}
fn learn(degree: u8, root: Option<u8>) -> Command {
    Command::LearnChord {
        source: SourceId(1),
        track: TrackId(1),
        degree,
        root,
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
fn settings(pitches: &[u8], root: u8) -> HarmonySettings {
    let mut settings = HarmonySettings::default();
    settings.learned[0] = Some(LearnedChord::new(pitches, root).unwrap());
    settings
}

#[test]
fn learning_preserves_unisons_excludes_other_sources_and_does_not_steal_live_notes() {
    let mut e = engine();
    for (id, pitch) in [67, 60, 60].into_iter().enumerate() {
        input(&mut e, 1, id as u64, 0, on(pitch));
    }
    input(&mut e, 2, 0, 0, on(99));
    input(
        &mut e,
        1,
        0,
        1,
        Gesture::Expression {
            expression: Expression {
                pitch: 2_000_000,
                pressure: 40_000,
                timbre: 50_000,
            },
        },
    );
    e.drain_output().for_each(drop);
    e.command(learn(0, Some(53))).unwrap();
    let learned = e.session().harmony.learned[0].unwrap();
    assert_eq!(learned.notes(), [60, 60, 67]);
    assert_eq!(
        learned.root(),
        53,
        "rootless voicing retains explicit harmonic root"
    );
    assert_eq!(e.snapshot().active_voices, 4);
    assert_eq!(e.drain_output().count(), 0);
    e.command(Command::ClearLearnedChord { degree: 0 }).unwrap();
    assert_eq!(e.session().harmony.learned[0], None);
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().harmony.learned[0], Some(learned));
    e.disconnect(SourceId(1)).unwrap();
    assert_eq!(e.snapshot().active_voices, 1);
    assert_eq!(e.drain_output().count(), 3);
    input(&mut e, 2, 0, 2, Gesture::Off { velocity: 7 });
    assert_eq!(e.snapshot().active_voices, 0);
}

#[test]
fn learned_voicings_use_existing_transforms_bass_root_and_diatonic_empty_pads() {
    let mut s = settings(&[50, 57, 61], 48);
    assert_eq!(s.voicing(0, None).unwrap().notes(), [50, 57, 61]);
    assert_eq!(s.voicing(1, None).unwrap().notes(), [62, 65, 69]);
    s.tonic = 72;
    s.scale = Scale::Dorian;
    s.extension = ChordExtension::Ninth;
    s.suspension = Suspension::Fourth;
    s.borrowed = true;
    assert_eq!(s.voicing(0, None).unwrap().notes(), [50, 57, 61]);
    s.inversion = 1;
    s.register = 1;
    s.spread = true;
    let voicing = s.voicing(0, None).unwrap();
    assert_eq!(voicing.notes(), [69, 74, 85]);
    assert_eq!(voicing.root(), 60);
    let mut single = settings(&[60], 60);
    single.spread = true;
    assert_eq!(single.voicing(0, None).unwrap().notes(), [60]);
    single.register = -3;
    single.learned[0] = Some(LearnedChord::new(&[12], 12).unwrap());
    assert!(
        single.validate().is_err(),
        "out-of-range transpose must reject, not clamp"
    );
}

#[test]
fn learned_strum_freezes_pending_pitches_and_expression_policy_per_trigger() {
    let mut e = engine();
    let mut s = settings(&[55, 62, 70], 55);
    s.articulation = Articulation::Strum {
        direction: StrumDirection::Ascending,
        interval: BeatTime(100),
    };
    s.expression = ExpressionPolicy::TopVoice;
    e.command(Command::SetHarmony(s)).unwrap();
    e.command(Command::SetMode(SurfaceMode::Chords)).unwrap();
    input(&mut e, 1, 0, 0, on(30));
    e.command(Command::ClearLearnedChord { degree: 0 }).unwrap();
    let expression = Expression {
        pitch: -300_000,
        pressure: 45_000,
        timbre: 42_000,
    };
    input(&mut e, 1, 0, 50, Gesture::Expression { expression });
    e.advance(BeatTime(200)).unwrap();
    let first: Vec<_> = e.drain_output().collect();
    assert_eq!(
        attacks(&first),
        [
            (0, 55, Expression::default()),
            (100, 62, Expression::default()),
            (200, 70, expression),
        ]
    );
    input(&mut e, 1, 1, 201, on(30));
    assert_eq!(attacks(&e.drain_output().collect::<Vec<_>>())[0].1, 60);
    input(&mut e, 1, 0, 202, Gesture::Off { velocity: 31 });
    let releases: Vec<_> = e.drain_output().collect();
    assert_eq!(releases.len(), 3);
    assert!(
        first
            .iter()
            .all(|on| releases.iter().any(|off| off.voice == on.voice))
    );
    e.command(Command::Stop).unwrap();
    assert_eq!(
        e.drain_output().count(),
        1,
        "unsounded notes have no phantom release"
    );
}

#[test]
fn learned_chord_recording_reopens_and_plays_with_independent_live_expression() {
    let mut e = engine();
    e.command(Command::SetHarmony(settings(&[58, 63, 69], 51)))
        .unwrap();
    e.command(Command::SetMode(SurfaceMode::Chords)).unwrap();
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    input(&mut e, 1, 0, 0, on(30));
    let expression = Expression {
        pitch: 700_000,
        pressure: 30_000,
        timbre: 55_000,
    };
    input(&mut e, 1, 0, 100, Gesture::Expression { expression });
    input(&mut e, 1, 0, 200, Gesture::Off { velocity: 42 });
    e.advance(BeatTime(300)).unwrap();
    e.command(Command::FinishRecord).unwrap();
    let bytes = serde_json::to_vec(e.session()).unwrap();
    let session: Session = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(session, *e.session());
    let clip = session.slots[0].clip.as_ref().unwrap();
    assert_eq!(clip.harmony_origin[0].algorithm_version, 3);
    assert_eq!(
        clip.harmony_origin[0].settings.learned[0],
        e.session().harmony.learned[0]
    );
    let mut replay = Engine::new(API_VERSION, session, Limits::default()).unwrap();
    replay
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    input(&mut replay, 1, 9, 0, on(58));
    let opening: Vec<_> = replay.drain_output().collect();
    assert_eq!(
        attacks(&opening).iter().map(|e| e.1).collect::<Vec<_>>(),
        [58, 63, 69, 58]
    );
    let live = opening[3].voice;
    replay.advance(BeatTime(100)).unwrap();
    let curves: Vec<_> = replay.drain_output().collect();
    assert_eq!(curves.len(), 3);
    assert!(curves.iter().all(|event| event.voice != live && event.gesture == Gesture::Expression { expression }));
    replay.advance(BeatTime(200)).unwrap();
    assert_eq!(replay.drain_output().count(), 3);
    assert_eq!(replay.snapshot().active_voices, 1);
    replay.command(Command::Stop).unwrap();
    assert_eq!(replay.drain_output().next().unwrap().voice, live);
}

#[test]
fn invalid_learning_keeps_bank_undo_and_output_and_never_truncates_a_chord() {
    let mut e = engine();
    assert!(e.command(learn(0, None)).is_err());
    input(&mut e, 1, 0, 0, on(60));
    e.command(learn(0, None)).unwrap();
    let before = e.session().clone();
    for id in 1..6 {
        input(&mut e, 1, id, id, on(60 + id as u8));
    }
    e.drain_output().for_each(drop);
    assert!(matches!(e.command(learn(0, None)), Err(Error::Capacity(_))));
    assert_eq!(*e.session(), before);
    assert_eq!(e.drain_output().count(), 0);
    assert_eq!(e.snapshot().active_voices, 6);
    assert!(e.command(Command::ClearLearnedChord { degree: 7 }).is_err());
    e.command(Command::Undo).unwrap();
    assert!(
        e.session().harmony.learned[0].is_none(),
        "failed learn must preserve prior Undo"
    );
    e.command(Command::SetMode(SurfaceMode::Chords)).unwrap();
    assert!(e.command(learn(0, None)).is_err());
}

#[test]
fn learned_chord_parser_and_schema_migration_reject_false_legacy_claims() {
    for json in [
        r#"{"notes":[],"root":60}"#,
        r#"{"notes":[128],"root":60}"#,
        r#"{"notes":[60],"root":128}"#,
        r#"{"notes":[60,61,62,63,64,65],"root":60}"#,
        r#"{"notes":[60],"root":60,"count":1}"#,
    ] {
        assert!(
            serde_json::from_str::<LearnedChord>(json).is_err(),
            "{json}"
        );
    }
    for version in 1..=3 {
        let mut old = serde_json::to_value(Session::default()).unwrap();
        old["version"] = version.into();
        old["harmony"].as_object_mut().unwrap().remove("learned");
        let migrated = serde_json::from_value::<Session>(old)
            .unwrap()
            .migrate()
            .unwrap();
        assert_eq!(migrated, Session::default());
        let false_legacy = Session {
            version,
            harmony: settings(&[60, 65], 60),
            ..Session::default()
        };
        assert!(false_legacy.migrate().is_err());
    }
    let mut pattern = PatternSettings::default()
        .render(settings(&[60, 65], 60), TrackId(1))
        .unwrap();
    assert_eq!(pattern.pattern_origin[0].algorithm_version, 2);
    let mut false_legacy = Session {
        version: 3,
        ..Session::default()
    };
    false_legacy.slots[0].clip = Some(pattern.clone());
    assert!(
        false_legacy.migrate().is_err(),
        "nested learned origin must also reject downgrade"
    );
    pattern.pattern_origin[0].algorithm_version = 1;
    assert!(pattern.validate(TrackId(1)).is_err());
    let mut old = Session {
        version: 3,
        ..Session::default()
    };
    let mut legacy = PatternSettings::default()
        .render(HarmonySettings::default(), TrackId(1))
        .unwrap();
    legacy.pattern_origin[0].algorithm_version = 1;
    old.slots[0].clip = Some(legacy.clone());
    let migrated = old.migrate().unwrap();
    assert_eq!(
        migrated.slots[0].clip,
        Some(legacy),
        "rendered events and original algorithm labels survive"
    );
}

#[test]
fn generators_and_harmony_followers_use_learned_notes_without_changing_rhythm() {
    let reference = settings(&[60, 64, 67], 60);
    let target = settings(&[58, 63, 69], 58);
    let follow = HarmonyFollow { reference };
    for (from, to) in [(60, 58), (64, 63), (67, 69), (72, 70)] {
        assert_eq!(follow.pitch(from, target).unwrap(), to);
    }
    let pattern = PatternSettings {
        tone_min: 0,
        tone_max: 2,
        ..PatternSettings::default()
    };
    let original = pattern.render(reference, TrackId(1)).unwrap();
    let learned = pattern.render(target, TrackId(1)).unwrap();
    assert_eq!(
        original.events.iter().map(|e| e.at).collect::<Vec<_>>(),
        learned.events.iter().map(|e| e.at).collect::<Vec<_>>()
    );
    for event in learned.events {
        if let Gesture::On { key, .. } = event.gesture {
            assert!([58, 63, 69].contains(&key));
        }
    }
}

#[test]
fn learning_uses_held_ownership_after_history_eviction_and_excludes_clips_and_other_tracks() {
    let mut session = Session::default();
    session.tracks.push(Track {
        id: TrackId(2),
        name: "Other".into(),
        instrument: session.tracks[0].instrument.clone(),
    });
    session.slots[0].clip = Some(
        PatternSettings {
            pulses: 1,
            gate: BeatTime(900_000),
            step: BeatTime(960_000),
            ..PatternSettings::default()
        }
        .render(HarmonySettings::default(), TrackId(1))
        .unwrap(),
    );
    let mut e = Engine::new(
        API_VERSION,
        session,
        Limits {
            voices: 8,
            output_events: 16,
            recorded_events: 8,
        },
    )
    .unwrap();
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(0),
    })
    .unwrap();
    input(&mut e, 1, 0, 0, on(63));
    e.input(InputEvent {
        at: BeatTime(0),
        note: NoteKey {
            source: SourceId(1),
            track: TrackId(2),
            note: NoteId(1),
        },
        gesture: on(95),
    })
    .unwrap();
    e.drain_output().for_each(drop);
    for at in 1..20 {
        input(
            &mut e,
            1,
            0,
            at,
            Gesture::Expression {
                expression: Expression::default(),
            },
        );
        e.drain_output().for_each(drop);
    }
    e.command(learn(0, None)).unwrap();
    assert_eq!(e.session().harmony.learned[0].unwrap().notes(), [63]);
    assert_eq!(e.snapshot().active_voices, 3);
    assert_eq!(e.drain_output().count(), 0);
}

#[test]
fn bank_undo_preserves_later_harmony_controls_and_retries_invalid_restoration() {
    let mut e = engine();
    input(&mut e, 1, 0, 0, on(60));
    e.command(learn(0, None)).unwrap();
    let mut s = e.session().harmony;
    s.tonic = 127; // The learned entry is valid; its diatonic replacement would exceed MIDI.
    s.articulation = Articulation::Strum {
        direction: StrumDirection::Descending,
        interval: BeatTime(100),
    };
    e.command(Command::SetHarmony(s)).unwrap();
    assert!(e.command(Command::Undo).is_err());
    assert_eq!(e.session().harmony, s);
    s.tonic = 62;
    e.command(Command::SetHarmony(s)).unwrap();
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().harmony.learned[0], None);
    assert_eq!(e.session().harmony.tonic, 62);
    assert_eq!(e.session().harmony.articulation, s.articulation);
    assert_eq!(e.snapshot().active_voices, 1);
}
