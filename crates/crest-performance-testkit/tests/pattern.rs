use crest_performance_core::*;

#[test]
fn euclidean_steps_rotation_density_and_lane_independence() {
    let settings = PatternSettings {
        steps: 8,
        pulses: 5,
        rotation: 0,
        seed: 0,
        ..PatternSettings::default()
    };
    let active: Vec<_> = (0..8).filter(|s| settings.has_pulse(*s).unwrap()).collect();
    assert_eq!(active, [0, 2, 4, 5, 7]);
    let rotated = PatternSettings {
        rotation: 1,
        ..settings
    };
    assert_eq!(
        (0..8)
            .filter(|s| rotated.has_pulse(*s).unwrap())
            .collect::<Vec<_>>(),
        [0, 1, 3, 5, 6]
    );
    let harmony = HarmonySettings::default();
    let sparse = settings.render(harmony, TrackId(1)).unwrap();
    let dense = PatternSettings {
        pulses: 8,
        ..settings
    }
    .render(harmony, TrackId(1))
    .unwrap();
    let dense_notes = dense.notes().unwrap();
    for note in sparse.notes().unwrap() {
        let same = dense_notes.iter().find(|n| n.start == note.start).unwrap();
        assert_eq!(
            note.key, same.key,
            "density must not shift pitch-lane random choices"
        );
    }
    assert_eq!(
        dense_notes[0].key, 60,
        "seed zero starts at chord tone zero"
    );
    assert_eq!(sparse, settings.render(harmony, TrackId(1)).unwrap());
    assert_ne!(
        sparse.events,
        PatternSettings {
            seed: 77,
            ..settings
        }
        .render(harmony, TrackId(1))
        .unwrap()
        .events
    );
}
#[test]
fn humanization_is_bounded_and_invalid_generation_is_atomic() {
    let settings = PatternSettings {
        steps: 128,
        pulses: 128,
        step: BeatTime(1000),
        gate: BeatTime(600),
        timing_variation: BeatTime(400),
        velocity: 70,
        velocity_variation: 20,
        ..PatternSettings::default()
    };
    let clip = settings
        .render(HarmonySettings::default(), TrackId(1))
        .unwrap();
    for (step, note) in clip.notes().unwrap().into_iter().enumerate() {
        assert!((step as u64 * 1000..=step as u64 * 1000 + 400).contains(&note.start.0));
        assert!((50..=90).contains(&note.velocity));
        assert_eq!(note.duration, BeatTime(600));
        assert!([60, 64, 67, 72, 76, 79].contains(&note.key));
    }
    assert_eq!(
        clip.pattern_origin[0].algorithm_version,
        PATTERN_ALGORITHM_VERSION
    );
    let mut e = Engine::new(API_VERSION, Session::default(), Limits::default()).unwrap();
    e.command(Command::Generate { slot: 0, settings }).unwrap();
    let before = e.session().clone();
    assert!(
        e.command(Command::Generate {
            slot: 0,
            settings: PatternSettings {
                timing_variation: BeatTime(401),
                ..settings
            }
        })
        .is_err()
    );
    assert_eq!(e.session(), &before);
    let saved: Session = serde_json::from_slice(&serde_json::to_vec(&before).unwrap()).unwrap();
    assert_eq!(saved, before);
    assert!(
        PatternSettings {
            steps: 129,
            ..settings
        }
        .validate()
        .is_err()
    );
    assert!(
        PatternSettings {
            step: BeatTime(u64::MAX),
            ..settings
        }
        .validate()
        .is_err()
    );
    assert!(
        PatternSettings {
            tone_min: -16,
            tone_max: 16,
            ..settings
        }
        .render(
            HarmonySettings {
                tonic: 0,
                ..HarmonySettings::default()
            },
            TrackId(1)
        )
        .is_err()
    );
}
fn attacks(e: &mut Engine) -> Vec<OutputEvent> {
    e.drain_output()
        .filter(|e| matches!(e.gesture, Gesture::On { .. }))
        .collect()
}
fn pitch(e: &OutputEvent) -> u8 {
    if let Gesture::On { key, .. } = e.gesture {
        key
    } else {
        panic!("expected attack")
    }
}
#[test]
fn follower_changes_only_future_attacks_and_freeze_undo_preserves_original() {
    let mut e = Engine::new(API_VERSION, Session::default(), Limits::default()).unwrap();
    let settings = PatternSettings {
        steps: 4,
        pulses: 4,
        step: BeatTime(1000),
        gate: BeatTime(900),
        tone_min: 0,
        tone_max: 0,
        follow_harmony: true,
        ..PatternSettings::default()
    };
    e.command(Command::Generate { slot: 0, settings }).unwrap();
    let original = e.session().slots[0].clip.clone().unwrap();
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(0),
    })
    .unwrap();
    e.advance(BeatTime(0)).unwrap();
    let first = attacks(&mut e);
    assert_eq!(pitch(&first[0]), 60);
    e.advance(BeatTime(100)).unwrap();
    e.command(Command::SetHarmony(HarmonySettings {
        degree: 1,
        ..HarmonySettings::default()
    }))
    .unwrap();
    assert_eq!(
        e.drain_output().count(),
        0,
        "harmony change must not retune held notes"
    );
    e.advance(BeatTime(900)).unwrap();
    let release = e.drain_output().next().unwrap();
    assert_eq!(release.voice, first[0].voice);
    assert!(matches!(release.gesture, Gesture::Off { .. }));
    e.advance(BeatTime(1000)).unwrap();
    let second = attacks(&mut e);
    assert_eq!(pitch(&second[0]), 62);
    assert_eq!(second[0].at, BeatTime(1000));
    e.command(Command::StopSlot { slot: 0 }).unwrap();
    e.command(Command::FreezeHarmony { slot: 0 }).unwrap();
    let frozen = e.session().slots[0].clip.clone().unwrap();
    assert!(frozen.follow.is_none());
    assert!(frozen.notes().unwrap().iter().all(|n| n.key == 62));
    assert_eq!(frozen.pattern_origin, original.pattern_origin);
    e.command(Command::Undo).unwrap();
    assert_eq!(e.session().slots[0].clip.as_ref(), Some(&original));
    assert!(e.command(Command::BeginOverdub { slot: 0 }).is_err());
}
#[test]
fn follower_preserves_chromatic_offsets_and_rejects_out_of_range_mapping() {
    let follow = HarmonyFollow {
        reference: HarmonySettings::default(),
    };
    let d_minor = HarmonySettings {
        degree: 1,
        ..HarmonySettings::default()
    };
    assert_eq!(follow.pitch(60, d_minor).unwrap(), 62);
    assert_eq!(follow.pitch(64, d_minor).unwrap(), 65);
    assert_eq!(follow.pitch(67, d_minor).unwrap(), 69);
    assert_eq!(follow.pitch(61, d_minor).unwrap(), 63);
    assert_eq!(follow.pitch(48, d_minor).unwrap(), 50);
    assert!(
        follow
            .pitch(
                127,
                HarmonySettings {
                    tonic: 100,
                    ..d_minor
                }
            )
            .is_err()
    );
}
#[test]
fn generated_clips_can_run_as_independent_lanes_and_undo_replacement() {
    let mut e = Engine::new(API_VERSION, Session::default(), Limits::default()).unwrap();
    let a = PatternSettings {
        steps: 3,
        pulses: 3,
        step: BeatTime(100),
        gate: BeatTime(50),
        tone_min: 0,
        tone_max: 0,
        ..PatternSettings::default()
    };
    let b = PatternSettings {
        steps: 5,
        pulses: 2,
        step: BeatTime(100),
        gate: BeatTime(50),
        tone_min: 1,
        tone_max: 1,
        ..a
    };
    e.command(Command::Generate {
        slot: 0,
        settings: a,
    })
    .unwrap();
    e.command(Command::Generate {
        slot: 1,
        settings: b,
    })
    .unwrap();
    for slot in [0, 1] {
        e.command(Command::Launch {
            slot,
            quantum: BeatTime(0),
        })
        .unwrap();
    }
    e.advance(BeatTime(1499)).unwrap();
    let played = attacks(&mut e);
    assert_eq!(played.iter().filter(|n| pitch(n) == 60).count(), 15);
    assert_eq!(played.iter().filter(|n| pitch(n) == 64).count(), 6);
    assert_eq!(e.snapshot().slots[0], SlotState::Playing);
    assert_eq!(e.snapshot().slots[1], SlotState::Playing);
    e.command(Command::Stop).unwrap();
    e.command(Command::Generate {
        slot: 0,
        settings: b,
    })
    .unwrap();
    e.command(Command::Undo).unwrap();
    assert_eq!(
        e.session().slots[0].clip.as_ref().unwrap().length,
        BeatTime(300)
    );
}
