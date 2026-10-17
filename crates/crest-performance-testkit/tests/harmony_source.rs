use crest_performance_core::*;

fn event(track: u32, id: u64, at: u64, gesture: Gesture) -> InputEvent {
    InputEvent {
        at: BeatTime(at),
        note: NoteKey {
            source: SourceId(1),
            track: TrackId(track),
            note: NoteId(id),
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
fn origin(at: u64, degree: u8) -> HarmonyGesture {
    HarmonyGesture {
        algorithm_version: HARMONY_ALGORITHM_VERSION,
        input: event(1, at, at, on(30 + degree)),
        settings: HarmonySettings {
            degree,
            ..HarmonySettings::default()
        },
    }
}
fn session() -> Session {
    let mut s = Session::default();
    s.harmony.degree = 1; // D minor is the live/selected fallback.
    s.harmony_source = HarmonySource::Clip { slot: 0 };
    s.tracks.push(Track {
        id: TrackId(2),
        name: "Follower".into(),
        instrument: InstrumentTarget {
            destination: DestinationId(2),
            reference: "capture://follower".into(),
        },
    });
    s.slots[0].clip = Some(Clip {
        length: BeatTime(20),
        // Rendered attacks deliberately lag the source gestures, as a strum can.
        events: vec![
            event(1, 1, 4, on(60)),
            event(1, 1, 6, Gesture::Off { velocity: 0 }),
            event(1, 2, 14, on(67)),
            event(1, 2, 16, Gesture::Off { velocity: 0 }),
        ],
        harmony_origin: vec![origin(2, 0), origin(12, 4)],
        pattern_origin: vec![],
        follow: None,
        conditions: vec![],
    });
    s.slots[1].track = TrackId(2);
    s.slots[1].clip = Some(Clip {
        length: BeatTime(5),
        events: vec![
            event(2, 1, 0, on(60)),
            event(
                2,
                1,
                3,
                Gesture::Expression {
                    expression: Expression {
                        pitch: 100_000,
                        pressure: 456,
                        timbre: 789,
                    },
                },
            ),
            event(2, 1, 4, Gesture::Off { velocity: 8 }),
        ],
        harmony_origin: vec![],
        pattern_origin: vec![],
        follow: Some(HarmonyFollow {
            reference: HarmonySettings::default(),
        }),
        conditions: vec![],
    });
    s
}
fn engine(s: Session) -> Engine {
    Engine::new(API_VERSION, s, Limits::default()).unwrap()
}
fn launch(e: &mut Engine, slot: usize) {
    e.command(Command::Launch {
        slot,
        quantum: BeatTime(0),
    })
    .unwrap();
}
fn follower(events: &[OutputEvent]) -> Vec<(u64, u8)> {
    events
        .iter()
        .filter_map(|event| match event.gesture {
            Gesture::On { key, .. } if event.track == TrackId(2) => Some((event.at.0, key)),
            _ => None,
        })
        .collect()
}

#[test]
fn leader_intent_precedes_delayed_audio_preserves_rhythm_and_is_launch_order_independent() {
    for slots in [[0, 1], [1, 0]] {
        verify_timing_for_launch_order(slots);
    }
}

fn verify_timing_for_launch_order(slots: [usize; 2]) {
    let mut e = engine(session());
    for slot in slots {
        launch(&mut e, slot);
    }
    e.advance(BeatTime(39)).unwrap();
    let output: Vec<_> = e.drain_output().collect();
    assert_eq!(
        follower(&output),
        [
            (0, 62),
            (5, 60),
            (10, 60),
            (15, 67),
            (20, 67),
            (25, 60),
            (30, 60),
            (35, 67)
        ]
    );
    let events: Vec<_> = output
        .iter()
        .filter(|event| event.track == TrackId(2))
        .collect();
    for (index, note) in events.chunks_exact(3).enumerate() {
        assert_eq!(note[1].at, BeatTime(index as u64 * 5 + 3));
        assert_eq!(note[2].at, BeatTime(index as u64 * 5 + 4));
        assert_eq!(note[0].voice, note[1].voice);
        assert_eq!(note[0].voice, note[2].voice);
        assert!(
            matches!(note[1].gesture, Gesture::Expression { expression } if expression.pitch == 100_000)
        );
        assert_eq!(note[2].gesture, Gesture::Off { velocity: 8 });
    }
    assert_eq!(e.snapshot().effective_harmony.degree, 4);
    assert_eq!(e.snapshot().active_voices, 0);
    e.command(Command::Stop).unwrap();
    assert_eq!(e.snapshot().effective_harmony.degree, 1);
}

#[test]
fn queued_source_relaunch_stop_and_live_selection_have_explicit_boundaries() {
    let mut e = engine(session());
    launch(&mut e, 1);
    e.advance(BeatTime(1)).unwrap();
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(10),
    })
    .unwrap();
    e.advance(BeatTime(25)).unwrap();
    assert_eq!(
        follower(&e.drain_output().collect::<Vec<_>>()),
        [(0, 62), (5, 62), (10, 62), (15, 60), (20, 60), (25, 67)]
    );
    e.command(Command::SetHarmony(HarmonySettings {
        degree: 2,
        ..HarmonySettings::default()
    }))
    .unwrap();
    assert_eq!(e.snapshot().effective_harmony.degree, 4);
    e.command(Command::StopSlot { slot: 0 }).unwrap();
    assert_eq!(e.snapshot().effective_harmony.degree, 2);
    assert!(e.drain_output().all(|event| event.track == TrackId(1))); // Only leader releases.
    e.advance(BeatTime(30)).unwrap();
    assert_eq!(follower(&e.drain_output().collect::<Vec<_>>()), [(30, 64)]);
    launch(&mut e, 0); // Restart source; first pickup uses selected harmony again.
    assert_eq!(e.snapshot().effective_harmony.degree, 2);
    e.advance(BeatTime(35)).unwrap();
    assert_eq!(follower(&e.drain_output().collect::<Vec<_>>()), [(35, 60)]);
}

#[test]
fn same_tick_and_unsorted_origins_choose_last_recorded_gesture_without_reordering_audio() {
    let mut s = session();
    let clip = s.slots[0].clip.as_mut().unwrap();
    clip.harmony_origin = vec![origin(10, 4), origin(0, 0), origin(0, 3)];
    let original = clip.events.clone();
    let mut e = engine(s);
    launch(&mut e, 0);
    launch(&mut e, 1);
    e.advance(BeatTime(10)).unwrap();
    assert_eq!(
        follower(&e.drain_output().collect::<Vec<_>>()),
        [(0, 65), (5, 65), (10, 67)]
    );
    assert_eq!(e.session().slots[0].clip.as_ref().unwrap().events, original);
}

#[test]
fn freeze_generation_replacement_and_undo_use_current_leader_and_refresh_index() {
    let mut e = engine(session());
    let original = e.session().slots[1].clip.clone();
    launch(&mut e, 0);
    e.advance(BeatTime(15)).unwrap();
    e.command(Command::FreezeHarmony { slot: 1 }).unwrap();
    assert_eq!(
        e.session().slots[1].clip.as_ref().unwrap().notes().unwrap()[0].key,
        67
    );
    assert!(e.session().slots[1].clip.as_ref().unwrap().follow.is_none());
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().slots[1].clip, original);
    e.command(Command::Generate {
        slot: 2,
        settings: PatternSettings {
            tone_min: 0,
            tone_max: 0,
            ..PatternSettings::default()
        },
    })
    .unwrap();
    assert!(
        e.session().slots[2]
            .clip
            .as_ref()
            .unwrap()
            .notes()
            .unwrap()
            .iter()
            .all(|note| note.key == 67)
    );
    e.command(Command::StopSlot { slot: 0 }).unwrap();
    e.command(Command::Generate {
        slot: 0,
        settings: PatternSettings::default(),
    })
    .unwrap();
    launch(&mut e, 0);
    e.advance(BeatTime(18)).unwrap();
    assert_eq!(e.snapshot().effective_harmony.degree, 1); // No recorded chord intent.
    e.command(Command::StopSlot { slot: 0 }).unwrap();
    e.command(Command::Undo).unwrap();
    launch(&mut e, 0);
    e.advance(BeatTime(20)).unwrap();
    assert_eq!(e.snapshot().effective_harmony.degree, 0); // Restored index.
}

#[test]
fn persistence_preserves_source_but_reopens_stopped_and_legacy_labels_are_checked() {
    let mut e = engine(session());
    launch(&mut e, 0);
    e.advance(BeatTime(15)).unwrap();
    let saved = e.save_session().unwrap();
    let restored: Session = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    let mut reopened = engine(restored);
    assert_eq!(reopened.session(), &saved);
    assert_eq!(reopened.snapshot().effective_harmony.degree, 1);
    launch(&mut reopened, 0);
    reopened.advance(BeatTime(12)).unwrap();
    assert_eq!(reopened.snapshot().effective_harmony.degree, 4);
    for version in 1..=7 {
        let old = Session {
            version,
            ..Session::default()
        };
        assert_eq!(
            old.migrate().unwrap().harmony_source,
            HarmonySource::default()
        );
        let false_label = Session {
            version,
            harmony_source: HarmonySource::Clip { slot: 0 },
            ..Session::default()
        };
        assert!(false_label.migrate().is_err());
    }
    let before = reopened.session().clone();
    assert_eq!(
        reopened.command(Command::SetHarmonySource(HarmonySource::Clip { slot: 8 })),
        Err(Error::NotFound)
    );
    assert_eq!(reopened.session(), &before);
    assert!(
        Engine::new(
            API_VERSION,
            Session {
                harmony_source: HarmonySource::Clip { slot: usize::MAX },
                ..Session::default()
            },
            Limits::default()
        )
        .is_err()
    );
    assert!(serde_json::from_str::<HarmonySource>(r#"{"type":"selected","slot":0}"#).is_err());
}

#[test]
fn silent_conditional_leader_keeps_intent_and_source_switch_does_not_rewrite_notes() {
    let mut s = session();
    let clip = s.slots[0].clip.as_ref().unwrap();
    let mut silent = clip.clone();
    for note in clip.notes().unwrap() {
        silent = silent
            .with_note_condition(note.id, NoteCondition::Fill {})
            .unwrap();
    }
    s.slots[0].clip = Some(silent);
    let mut e = engine(s.clone());
    launch(&mut e, 0);
    launch(&mut e, 1);
    e.advance(BeatTime(19)).unwrap();
    let output: Vec<_> = e.drain_output().collect();
    assert!(output.iter().all(|event| event.track == TrackId(2)));
    assert_eq!(follower(&output), [(0, 62), (5, 60), (10, 60), (15, 67)]);
    e.command(Command::SetHarmonySource(HarmonySource::Selected {}))
        .unwrap();
    e.advance(BeatTime(20)).unwrap();
    assert_eq!(follower(&e.drain_output().collect::<Vec<_>>()), [(20, 62)]);
    assert_eq!(e.session().slots, s.slots);
}
