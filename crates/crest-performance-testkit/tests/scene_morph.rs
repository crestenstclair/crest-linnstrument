use crest_performance_core::*;

fn address(parameter: u32) -> ParameterAddress {
    ParameterAddress {
        destination: DestinationId(1),
        parameter,
    }
}
fn scene() -> SceneMorph {
    vec![
        MorphParameter {
            address: address(1),
            a: 0,
            b: 65_535,
        },
        MorphParameter {
            address: address(2),
            a: 60_000,
            b: 10_000,
        },
        MorphParameter {
            address: address(3),
            a: 123,
            b: 123,
        },
    ]
    .try_into()
    .unwrap()
}
fn engine(capacity: usize) -> Engine {
    Engine::new(
        API_VERSION,
        Session::default(),
        Limits {
            voices: 1,
            output_events: capacity,
            recorded_events: 64,
        },
    )
    .unwrap()
}
#[test]
fn interpolation_has_exact_endpoints_rounding_and_monotone_reverse_lane() {
    let scene = scene();
    for (position, expected) in [
        (0, [0, 60_000, 123]),
        (32_768, [32_768, 35_000, 123]),
        (65_535, [65_535, 10_000, 123]),
    ] {
        let events: Vec<_> = scene.render(BeatTime(17), position).collect();
        assert_eq!(events.iter().map(|e| e.value).collect::<Vec<_>>(), expected);
        assert!(events.iter().all(|e| e.at == BeatTime(17)));
    }
    let mut previous = [0, 60_000, 123];
    for position in 0..=u16::MAX {
        let values: Vec<_> = scene
            .render(BeatTime(0), position)
            .map(|e| e.value)
            .collect();
        assert!(values[0] >= previous[0] && values[1] <= previous[1]);
        previous.copy_from_slice(&values);
    }
}
#[test]
fn scene_validation_rejects_duplicates_unsorted_and_unbounded_input() {
    assert!(SceneMorph::try_from(vec![]).is_err());
    let parameter = MorphParameter {
        address: address(1),
        a: 0,
        b: 1,
    };
    assert!(SceneMorph::try_from(vec![parameter; 2]).is_err());
    assert!(SceneMorph::try_from(vec![parameter; 17]).is_err());
    let mut parameters = scene().parameters().to_vec();
    parameters.reverse();
    assert!(SceneMorph::try_from(parameters).is_err());
    assert!(
        serde_json::from_str::<SceneMorph>(
            r#"[{"address":{"destination":1,"parameter":1},"a":0,"b":65536}]"#
        )
        .is_err()
    );
}
#[test]
fn capability_rejection_and_queue_pressure_are_atomic_and_do_not_consume_release_space() {
    let mut engine = engine(3);
    engine.set_scene_morph(Some(scene())).unwrap();
    assert!(engine.command(Command::SetMorphPosition(2)).is_err());
    assert_eq!(engine.snapshot().morph_position, None);
    assert_eq!(engine.drain_parameter_output().count(), 0);
    engine
        .set_parameter_capabilities(&[address(1), address(2), address(3)])
        .unwrap();
    engine.command(Command::SetMorphPosition(123)).unwrap();
    assert_eq!(
        engine.command(Command::SetMorphPosition(456)),
        Err(Error::Capacity("parameter output"))
    );
    assert_eq!(engine.snapshot().morph_position, Some(123));
    let note = NoteKey {
        source: SourceId(1),
        track: TrackId(1),
        note: NoteId(1),
    };
    engine
        .input(InputEvent {
            at: BeatTime(0),
            note,
            gesture: Gesture::On {
                key: 60,
                velocity: 100,
                expression: Expression::default(),
            },
        })
        .unwrap();
    engine.advance(BeatTime(1)).unwrap();
    engine
        .input(InputEvent {
            at: BeatTime(1),
            note,
            gesture: Gesture::Off { velocity: 0 },
        })
        .unwrap();
    assert_eq!(engine.drain_output().count(), 2);
    assert!(engine.set_scene_morph(None).is_err());
    assert!(engine.set_parameter_capabilities(&[]).is_err());
    let captured: Vec<_> = engine.drain_parameter_output().collect();
    assert_eq!(
        captured,
        scene().render(BeatTime(0), 123).collect::<Vec<_>>()
    );
    engine.command(Command::SetMorphPosition(456)).unwrap();
    assert!(engine.drain_parameter_output().all(|e| e.at == BeatTime(1)));
    engine.set_parameter_capabilities(&[]).unwrap();
    assert!(engine.command(Command::SetMorphPosition(1)).is_err());
}
#[test]
fn persistence_upgrades_without_emitting_or_assuming_backend_capabilities() {
    let legacy = Session {
        version: 8,
        ..Session::default()
    };
    let before = serde_json::to_vec(&legacy).unwrap();
    let mut session: Session = serde_json::from_slice(&before).unwrap();
    session = session.migrate().unwrap();
    assert_eq!(session.version, SESSION_VERSION);
    assert_eq!(serde_json::to_vec(&legacy).unwrap(), before);
    session.scene_morph = Some(scene());
    let encoded = serde_json::to_vec(&session).unwrap();
    let saved: Session = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(session, saved);
    let mut reopened = Engine::new(API_VERSION, saved, Limits::default()).unwrap();
    assert_eq!(reopened.drain_parameter_output().count(), 0);
    assert!(reopened.command(Command::SetMorphPosition(0)).is_err());
    let mut relabeled = session;
    relabeled.version = 8;
    assert!(relabeled.migrate().is_err());
}
#[test]
fn nonexistent_destination_rejects_without_replacing_previous_scene() {
    let mut engine = engine(16);
    engine.set_scene_morph(Some(scene())).unwrap();
    let unknown = ParameterAddress {
        destination: DestinationId(99),
        parameter: 1,
    };
    let next = vec![MorphParameter {
        address: unknown,
        a: 0,
        b: 1,
    }]
    .try_into()
    .unwrap();
    assert!(engine.set_scene_morph(Some(next)).is_err());
    assert_eq!(engine.session().scene_morph, Some(scene()));
    assert!(engine.set_parameter_capabilities(&[unknown]).is_err());
    assert!(engine.snapshot().parameter_capabilities.is_empty());
}
