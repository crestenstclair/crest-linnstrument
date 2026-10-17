use crest_performance_core::*;

fn event(key: u8, at: u64, gesture: Gesture) -> InputEvent {
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
    let mut s = Session::default();
    for (slot, key) in [(0, 60), (1, 67)] {
        s.slots[slot].clip = Some(Clip {
            length: BeatTime(20),
            events: vec![
                event(key, 0, on(key)),
                event(
                    key,
                    5,
                    Gesture::Expression {
                        expression: Expression {
                            pitch: 234_567,
                            ..Expression::default()
                        },
                    },
                ),
                event(key, 15, Gesture::Off { velocity: 8 }),
            ],
            harmony_origin: vec![],
            pattern_origin: vec![],
            follow: None,
            conditions: vec![],
            parameter_locks: Vec::new(),
        });
    }
    s
}
fn engine() -> Engine {
    Engine::new(API_VERSION, session(), Limits::default()).unwrap()
}
fn launch(e: &mut Engine, slot: usize) {
    e.command(Command::Launch {
        slot,
        quantum: BeatTime(0),
    })
    .unwrap();
}
fn mute(e: &mut Engine, slot: usize, muted: bool) {
    e.command(Command::SetClipMuted { slot, muted }).unwrap();
}

#[test]
fn mute_releases_only_selected_clip_and_unmute_keeps_phase_without_reviving_old_notes() {
    let mut e = engine();
    launch(&mut e, 0);
    launch(&mut e, 1);
    e.advance(BeatTime(0)).unwrap();
    e.input(event(72, 0, on(72))).unwrap();
    let initial: Vec<_> = e.drain_output().collect();
    assert_eq!(initial.len(), 3);
    e.advance(BeatTime(3)).unwrap();
    mute(&mut e, 0, true);
    let release: Vec<_> = e.drain_output().collect();
    assert_eq!(release.len(), 1);
    assert_eq!(release[0].voice, initial[0].voice);
    assert_eq!(release[0].at, BeatTime(3));
    assert_eq!(release[0].gesture, Gesture::Off { velocity: 0 });
    assert_eq!(e.snapshot().active_voices, 2);
    assert_eq!(&e.snapshot().muted_slots[..2], &[true, false]);
    e.advance(BeatTime(10)).unwrap();
    let expression: Vec<_> = e.drain_output().collect();
    assert_eq!(expression.len(), 1);
    assert_eq!(expression[0].voice, initial[1].voice);
    mute(&mut e, 0, false);
    assert_eq!(e.drain_output().count(), 0);
    e.advance(BeatTime(15)).unwrap();
    let releases: Vec<_> = e.drain_output().collect();
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].voice, initial[1].voice);
    e.advance(BeatTime(20)).unwrap();
    let resumed: Vec<_> = e.drain_output().collect();
    assert_eq!(resumed.len(), 2);
    assert!(resumed.iter().all(|event| event.at == BeatTime(20)));
    assert_eq!(resumed[0].gesture, on(60));
    assert_ne!(resumed[0].voice, initial[0].voice);
    e.command(Command::Stop).unwrap();
    assert_eq!(e.snapshot().active_voices, 0);
}

#[test]
fn queued_mute_suppresses_attack_and_release_without_shifting_launch_boundary() {
    let mut e = engine();
    e.advance(BeatTime(1)).unwrap();
    e.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(10),
    })
    .unwrap();
    mute(&mut e, 0, true);
    assert_eq!(e.snapshot().slots[0], SlotState::Queued);
    assert_eq!(e.slot_launch_time(0).unwrap(), Some(BeatTime(10)));
    e.advance(BeatTime(26)).unwrap();
    assert_eq!(e.drain_output().count(), 0);
    assert_eq!(e.snapshot().slots[0], SlotState::Playing);
    mute(&mut e, 0, false);
    e.advance(BeatTime(30)).unwrap();
    let output: Vec<_> = e.drain_output().collect();
    assert_eq!(output.len(), 1);
    assert_eq!(output[0].at, BeatTime(30));
    assert_eq!(output[0].gesture, on(60));
}

#[test]
fn mute_capacity_error_is_atomic_and_retry_preserves_release() {
    let mut e = Engine::new(
        API_VERSION,
        session(),
        Limits {
            voices: 1,
            output_events: 1,
            recorded_events: 2,
        },
    )
    .unwrap();
    launch(&mut e, 0);
    e.advance(BeatTime(0)).unwrap();
    assert!(matches!(
        e.command(Command::SetClipMuted {
            slot: 0,
            muted: true
        }),
        Err(Error::Capacity(_))
    ));
    assert!(!e.snapshot().muted_slots[0]);
    assert_eq!(e.snapshot().active_voices, 1);
    let voice = e.drain_output().next().unwrap().voice;
    mute(&mut e, 0, true);
    let release = e.drain_output().next().unwrap();
    assert_eq!(release.voice, voice);
    assert_eq!(release.gesture, Gesture::Off { velocity: 0 });
    e.advance(BeatTime(100)).unwrap();
    assert_eq!(e.drain_output().count(), 0);
}

#[test]
fn safety_transitions_clear_mute_and_invalid_commands_do_not_change_state() {
    for command in [
        Command::Stop,
        Command::StopSlot { slot: 0 },
        Command::SetMode(SurfaceMode::Clips),
    ] {
        let mut e = engine();
        launch(&mut e, 0);
        mute(&mut e, 0, true);
        e.command(command).unwrap();
        assert!(!e.snapshot().muted_slots[0]);
    }
    let mut e = engine();
    launch(&mut e, 0);
    mute(&mut e, 0, true);
    e.disconnect(SourceId(1)).unwrap();
    assert!(!e.snapshot().muted_slots[0]);
    mute(&mut e, 0, true);
    launch(&mut e, 0);
    assert!(!e.snapshot().muted_slots[0]);
    assert_eq!(
        e.command(Command::SetClipMuted {
            slot: 99,
            muted: true
        }),
        Err(Error::NotFound)
    );
    assert!(
        e.command(Command::SetClipMuted {
            slot: 2,
            muted: true
        })
        .is_err()
    );
    mute(&mut e, 2, false); // A delayed UI release after Stop is harmless.
}

#[test]
fn mute_is_transient_preserves_original_session_and_does_not_consume_edit_undo() {
    let mut e = engine();
    e.command(Command::Duplicate { from: 0, to: 2 }).unwrap();
    let before = e.save_session().unwrap();
    launch(&mut e, 0);
    mute(&mut e, 0, true);
    assert_eq!(e.save_session().unwrap(), before);
    let restored = Engine::new(API_VERSION, before, Limits::default()).unwrap();
    assert!(restored.snapshot().muted_slots.iter().all(|muted| !muted));
    e.command(Command::Undo).unwrap();
    assert!(e.session().slots[2].clip.is_none());
    assert!(e.snapshot().muted_slots[0]);
    assert_eq!(e.session(), &session());
}
