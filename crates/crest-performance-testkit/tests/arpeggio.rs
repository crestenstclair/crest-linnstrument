use crest_linnstrument::{MpeOutput, MpeZone};
use crest_performance_core::*;
use crest_performance_testkit::CaptureSink;
use std::collections::BTreeSet;

fn settings(direction: StrumDirection, gate: u64) -> HarmonySettings {
    HarmonySettings {
        articulation: Articulation::Arpeggio {
            direction,
            interval: BeatTime(100),
            gate: BeatTime(gate),
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
fn input(e: &mut Engine, at: u64, id: u64, gesture: Gesture) -> Result<()> {
    e.advance(BeatTime(at))?;
    e.input(InputEvent {
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
        velocity: 90,
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
fn balanced(events: &[OutputEvent]) {
    let mut held = BTreeSet::new();
    let mut used = BTreeSet::new();
    for event in events {
        match event.gesture {
            Gesture::On { .. } => {
                assert!(used.insert(event.voice.0));
                assert!(held.insert(event.voice.0));
            }
            Gesture::Off { .. } => assert!(held.remove(&event.voice.0)),
            Gesture::Expression { .. } => assert!(held.contains(&event.voice.0)),
        }
    }
    assert!(held.is_empty());
}

#[test]
fn repeated_tones_use_fresh_identities_and_release_before_equal_time_attacks() {
    for (direction, keys) in [
        (StrumDirection::Ascending, [60, 64, 67]),
        (StrumDirection::Descending, [67, 64, 60]),
    ] {
        let mut e = engine(settings(direction, 100), Limits::default());
        input(&mut e, 0, 1, on(30)).unwrap();
        e.advance(BeatTime(850)).unwrap();
        input(&mut e, 850, 1, Gesture::Off { velocity: 41 }).unwrap();
        e.advance(BeatTime(2000)).unwrap();
        let events: Vec<_> = e.drain_output().collect();
        let expected: Vec<_> = (0..9)
            .map(|i| (i * 100, keys[i as usize % 3], Expression::default()))
            .collect();
        assert_eq!(attacks(&events), expected);
        assert_eq!(events.len(), 18);
        for pair in events[1..17].chunks(2) {
            assert!(matches!(pair[0].gesture, Gesture::Off { velocity: 0 }));
            assert!(matches!(pair[1].gesture, Gesture::On { .. }));
            assert_eq!(pair[0].at, pair[1].at);
        }
        assert_eq!(
            events.last().unwrap().gesture,
            Gesture::Off { velocity: 41 }
        );
        balanced(&events);
        assert_eq!(e.snapshot().active_voices, 0);
    }
}

#[test]
fn expression_in_silent_gaps_reaches_only_its_trigger_and_held_settings_stay_fixed() {
    let mut s = settings(StrumDirection::Ascending, 40);
    for (degree, key) in [60, 72, 84].into_iter().enumerate() {
        s.learned[degree] = Some(LearnedChord::new(&[key], key).unwrap());
    }
    let mut e = engine(s, Limits::default());
    for id in 1..=3 {
        input(&mut e, 0, id, on(29 + id as u8)).unwrap();
    }
    e.advance(BeatTime(50)).unwrap();
    let mut events: Vec<_> = e.drain_output().collect();
    let dimensions = [
        Expression {
            pitch: 300_000,
            ..Expression::default()
        },
        Expression {
            timbre: 55_000,
            ..Expression::default()
        },
        Expression {
            pressure: 45_000,
            ..Expression::default()
        },
    ];
    for (i, expression) in dimensions.into_iter().enumerate() {
        input(&mut e, 50, i as u64 + 1, Gesture::Expression { expression }).unwrap();
    }
    assert_eq!(
        e.drain_output().count(),
        0,
        "silent gates must not send orphan expression"
    );
    e.command(Command::SetHarmony(HarmonySettings::default()))
        .unwrap();
    e.advance(BeatTime(100)).unwrap();
    let next: Vec<_> = e.drain_output().collect();
    assert_eq!(
        attacks(&next),
        [
            (100, 60, dimensions[0]),
            (100, 72, dimensions[1]),
            (100, 84, dimensions[2])
        ]
    );
    events.extend(next);
    for id in 1..=3 {
        input(&mut e, 110, id, Gesture::Off { velocity: 0 }).unwrap();
    }
    events.extend(e.drain_output());
    balanced(&events);
}

#[test]
fn top_voice_policy_bass_and_manual_strumplate_do_not_inherit_repeating_chord_rules() {
    let mut s = settings(StrumDirection::Descending, 50);
    s.expression = ExpressionPolicy::TopVoice;
    s.bass_track = Some(TrackId(2));
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
    input(&mut e, 0, 1, on(30)).unwrap();
    let expression = Expression {
        pitch: 200_000,
        timbre: 50_000,
        pressure: 40_000,
    };
    input(&mut e, 25, 1, Gesture::Expression { expression }).unwrap();
    e.advance(BeatTime(400)).unwrap();
    let events: Vec<_> = e.drain_output().collect();
    assert_eq!(
        attacks(&events),
        [
            (0, 67, Expression::default()),
            (0, 48, Expression::default()),
            (100, 64, Expression::default()),
            (200, 60, Expression::default()),
            (300, 67, expression),
            (400, 64, Expression::default())
        ]
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.destination == DestinationId(2))
            .count(),
        1
    );
    e.command(Command::Stop).unwrap();
    let mut complete = events;
    complete.extend(e.drain_output());
    balanced(&complete);
    s.playing = HarmonyPlaying::Strumplate;
    e.command(Command::SetHarmony(s)).unwrap();
    input(&mut e, 500, 2, on(31)).unwrap();
    e.advance(BeatTime(2000)).unwrap();
    assert_eq!(
        attacks(&e.drain_output().collect::<Vec<_>>()),
        [(500, 64, Expression::default())]
    );
    input(&mut e, 2001, 2, Gesture::Off { velocity: 0 }).unwrap();
    assert_eq!(e.drain_output().count(), 1);
}

#[test]
fn release_stop_disconnect_and_mode_change_cancel_every_future_cycle_even_in_gaps() {
    for action in 0..4 {
        for at in [25, 75] {
            let mut e = engine(settings(StrumDirection::Ascending, 50), Limits::default());
            input(&mut e, 0, 1, on(30)).unwrap();
            e.advance(BeatTime(at)).unwrap();
            match action {
                0 => input(&mut e, at, 1, Gesture::Off { velocity: 0 }).unwrap(),
                1 => e.command(Command::Stop).unwrap(),
                2 => e.disconnect(SourceId(1)).unwrap(),
                _ => e.command(Command::SetMode(SurfaceMode::Play)).unwrap(),
            }
            e.advance(BeatTime(5000)).unwrap();
            let events: Vec<_> = e.drain_output().collect();
            assert_eq!(events.len(), 2);
            balanced(&events);
            assert_eq!(e.snapshot().active_voices, 0);
        }
    }
}

#[test]
fn output_pressure_retries_exact_events_and_full_recording_preserves_release_capacity() {
    let limits = Limits {
        voices: 3,
        output_events: 3,
        recorded_events: 6,
    };
    let mut e = engine(settings(StrumDirection::Ascending, 50), limits);
    input(&mut e, 0, 1, on(30)).unwrap();
    let mut events = Vec::new();
    while let Err(Error::Capacity(_)) = e.advance(BeatTime(375)) {
        events.extend(e.drain_output());
    }
    input(&mut e, 375, 1, Gesture::Off { velocity: 0 }).unwrap();
    events.extend(e.drain_output());
    assert_eq!(
        attacks(&events)
            .iter()
            .map(|e| (e.0, e.1))
            .collect::<Vec<_>>(),
        [(0, 60), (100, 64), (200, 67), (300, 60)]
    );
    balanced(&events);
    let mut e = engine(settings(StrumDirection::Ascending, 50), limits);
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    input(&mut e, 0, 1, on(30)).unwrap();
    e.drain_output().for_each(drop);
    e.advance(BeatTime(150)).unwrap();
    e.drain_output().for_each(drop);
    e.advance(BeatTime(250)).unwrap();
    e.drain_output().for_each(drop);
    assert!(matches!(e.advance(BeatTime(300)), Err(Error::Capacity(_))));
    assert_eq!(e.position(), BeatTime(250));
    assert_eq!(e.drain_output().count(), 0);
    // The take is full, but all gate releases have already succeeded.
    input(&mut e, 250, 1, Gesture::Off { velocity: 0 }).unwrap();
    e.command(Command::FinishRecord).unwrap();
    assert_eq!(e.session().slots[0].clip.as_ref().unwrap().events.len(), 6);
    assert_eq!(e.snapshot().active_voices, 0);
}

#[test]
fn recorded_cycles_reopen_and_replay_exact_curves_beside_live_mpe_notes() {
    let mut e = engine(settings(StrumDirection::Ascending, 80), Limits::default());
    e.command(Command::BeginRecord { slot: 0 }).unwrap();
    input(&mut e, 0, 1, on(30)).unwrap();
    input(
        &mut e,
        20,
        1,
        Gesture::Expression {
            expression: Expression {
                pitch: 220_000,
                timbre: 50_000,
                pressure: 30_000,
            },
        },
    )
    .unwrap();
    input(&mut e, 350, 1, Gesture::Off { velocity: 21 }).unwrap();
    e.advance(BeatTime(400)).unwrap();
    e.command(Command::FinishRecord).unwrap();
    let bytes = serde_json::to_vec(&e.save_session().unwrap()).unwrap();
    let reopened: Session = serde_json::from_slice(&bytes).unwrap();
    let clip = reopened.slots[0].clip.as_ref().unwrap().clone();
    assert_eq!(clip.notes().unwrap().len(), 4);
    assert_eq!(clip.harmony_origin.len(), 3);
    assert_eq!(
        clip.harmony_origin[0].algorithm_version,
        HARMONY_ALGORITHM_VERSION
    );
    let mut replay = Engine::new(API_VERSION, reopened, Limits::default()).unwrap();
    replay
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    input(&mut replay, 0, 99, on(60)).unwrap();
    let mut sink = CaptureSink::default();
    let mut output = MpeOutput::new(DestinationId(1), MpeZone::lower());
    sink.collect(&mut replay, &mut output).unwrap();
    let live = sink.events.last().unwrap().voice;
    replay.advance(BeatTime(399)).unwrap();
    sink.collect(&mut replay, &mut output).unwrap();
    let played: Vec<_> = sink
        .events
        .iter()
        .filter(|e| e.voice != live)
        .map(|e| (e.at, e.gesture))
        .collect();
    assert_eq!(
        played,
        clip.events
            .iter()
            .map(|e| (e.at, e.gesture))
            .collect::<Vec<_>>()
    );
    assert_eq!(replay.snapshot().active_voices, 1);
    replay.command(Command::Stop).unwrap();
    sink.collect(&mut replay, &mut output).unwrap();
    balanced(&sink.events);
}

#[test]
fn new_schema_rejects_false_legacy_labels_and_old_rendered_material_stays_exact() {
    for version in 1..=4 {
        let old = Session {
            version,
            ..Session::default()
        };
        assert_eq!(old.migrate().unwrap(), Session::default());
        let false_old = Session {
            version,
            harmony: settings(StrumDirection::Ascending, 50),
            ..Session::default()
        };
        assert!(false_old.migrate().is_err());
    }
    for (interval, gate) in [(0, 0), (100, 0), (100, 101), (TICKS_PER_BEAT + 1, 1)] {
        assert!(
            Articulation::Arpeggio {
                direction: StrumDirection::Ascending,
                interval: BeatTime(interval),
                gate: BeatTime(gate)
            }
            .validate()
            .is_err()
        );
    }
    let mut e = engine(settings(StrumDirection::Ascending, 50), Limits::default());
    assert!(input(&mut e, u64::MAX - 250, 1, on(30)).is_err());
    assert_eq!(e.drain_output().count(), 0);
    assert_eq!(e.snapshot().active_voices, 0);
    let mut clip = PatternSettings::default()
        .render(HarmonySettings::default(), TrackId(1))
        .unwrap();
    clip.pattern_origin[0].algorithm_version = 2;
    let old = Session {
        version: 4,
        slots: vec![ClipSlot {
            track: TrackId(1),
            clip: Some(clip.clone()),
        }],
        ..Session::default()
    };
    assert_eq!(old.migrate().unwrap().slots[0].clip, Some(clip.clone()));
    clip.pattern_origin[0].harmony = settings(StrumDirection::Ascending, 50);
    assert!(clip.validate(TrackId(1)).is_err());
}

#[test]
fn single_learned_top_voice_does_not_express_its_separately_routed_bass() {
    let mut s = settings(StrumDirection::Ascending, 50);
    s.learned[0] = Some(LearnedChord::new(&[60], 48).unwrap());
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
    input(&mut e, 0, 1, on(30)).unwrap();
    e.drain_output().for_each(drop);
    input(
        &mut e,
        25,
        1,
        Gesture::Expression {
            expression: Expression {
                pitch: 250_000,
                ..Expression::default()
            },
        },
    )
    .unwrap();
    let events: Vec<_> = e.drain_output().collect();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].destination, DestinationId(1));
    e.command(Command::Stop).unwrap();
    assert_eq!(e.drain_output().count(), 2);
}

#[test]
fn nested_arpeggio_provenance_cannot_be_downgraded_and_midcycle_time_exhaustion_still_releases() {
    let s = settings(StrumDirection::Ascending, 50);
    let mut clip = PatternSettings::default().render(s, TrackId(1)).unwrap();
    let old = Session {
        version: 4,
        slots: vec![ClipSlot {
            track: TrackId(1),
            clip: Some(clip.clone()),
        }],
        ..Session::default()
    };
    assert!(old.migrate().is_err());
    clip.pattern_origin.clear();
    clip.follow = Some(HarmonyFollow { reference: s });
    let old = Session {
        version: 4,
        slots: vec![ClipSlot {
            track: TrackId(1),
            clip: Some(clip.clone()),
        }],
        ..Session::default()
    };
    assert!(old.migrate().is_err());
    clip.follow = None;
    let origin = HarmonyGesture {
        algorithm_version: HARMONY_ALGORITHM_VERSION,
        input: InputEvent {
            at: BeatTime(0),
            note: NoteKey {
                source: SourceId(1),
                track: TrackId(1),
                note: NoteId(1),
            },
            gesture: on(30),
        },
        settings: s,
    };
    for algorithm_version in 1..=3 {
        assert!(
            HarmonyGesture {
                algorithm_version,
                ..origin
            }
            .validate()
            .is_err()
        );
    }
    clip.harmony_origin.push(origin);
    let old = Session {
        version: 4,
        slots: vec![ClipSlot {
            track: TrackId(1),
            clip: Some(clip),
        }],
        ..Session::default()
    };
    assert!(old.migrate().is_err());
    let mut e = engine(s, Limits::default());
    input(&mut e, u64::MAX - 550, 1, on(30)).unwrap();
    assert!(matches!(
        e.advance(BeatTime(u64::MAX)),
        Err(Error::Invalid(_))
    ));
    let events: Vec<_> = e.drain_output().collect();
    assert_eq!(attacks(&events).len(), 3);
    balanced(&events);
    e.command(Command::Stop).unwrap();
    assert_eq!(e.drain_output().count(), 0);
    assert_eq!(e.snapshot().active_voices, 0);
}
