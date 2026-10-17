use crest_performance_core::*;

#[test]
fn host_can_observe_launch_boundary_without_repeating_quantization_or_advancing_time() {
    let mut session = Session::default();
    session.slots[0].clip = Some(
        PatternSettings {
            steps: 1,
            pulses: 1,
            rotation: 0,
            step: BeatTime(20),
            gate: BeatTime(10),
            ..PatternSettings::default()
        }
        .render(HarmonySettings::default(), TrackId(1))
        .unwrap(),
    );
    let mut engine = Engine::new(API_VERSION, session, Limits::default()).unwrap();
    assert_eq!(engine.slot_launch_time(0).unwrap(), None);
    assert_eq!(engine.slot_launch_time(8), Err(Error::NotFound));
    engine.advance(BeatTime(13)).unwrap();
    engine
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(10),
        })
        .unwrap();
    assert_eq!(engine.slot_launch_time(0).unwrap(), Some(BeatTime(20)));
    assert_eq!(engine.position(), BeatTime(13));
    assert_eq!(engine.snapshot().slots[0], SlotState::Queued);
    assert_eq!(engine.drain_output().count(), 0);
    engine.advance(BeatTime(20)).unwrap();
    assert!(
        engine
            .drain_output()
            .any(|event| event.at == BeatTime(20) && matches!(event.gesture, Gesture::On { .. }))
    );
    engine.advance(BeatTime(101)).unwrap();
    assert_eq!(engine.slot_launch_time(0).unwrap(), Some(BeatTime(20)));
    let reopened = Engine::new(
        API_VERSION,
        engine.save_session().unwrap(),
        Limits::default(),
    )
    .unwrap();
    assert_eq!(reopened.slot_launch_time(0).unwrap(), None);
    engine.command(Command::StopSlot { slot: 0 }).unwrap();
    assert_eq!(engine.slot_launch_time(0).unwrap(), None);
    engine
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    assert_eq!(engine.slot_launch_time(0).unwrap(), Some(BeatTime(101)));
}
