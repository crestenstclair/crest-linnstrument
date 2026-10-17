use super::*;
use crest_performance_core::{API_VERSION, Articulation, Engine, Gesture, Limits};

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_humanized_chords_keep_seeded_timing_expression_and_exact_replay() {
    for model in [128, 200] {
        verify_humanized(model);
    }
}
fn verify_humanized(model: u16) {
    let mut r = super::harmony_tests::runtime(model);
    r.command(BridgeCommand::Harmony {
        settings: HarmonySettings {
            articulation: Articulation::Humanized {
                max_delay: BeatTime(60000),
                velocity_variation: 8,
                seed: 42,
            },
            ..HarmonySettings::default()
        },
    })
    .unwrap();
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    r.command(BridgeCommand::PlayExample {}).unwrap();
    r.advance(1_000_000).unwrap();
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    let session = r.host.engine.save_session().unwrap();
    let clip = session.slots[0].clip.as_ref().unwrap();
    let notes = clip.notes().unwrap();
    assert_eq!(notes.len(), 9);
    assert!(r.firmware.sensor_reads > 1000);
    let triggers: Vec<_> = clip
        .harmony_origin
        .iter()
        .filter(|origin| matches!(origin.input.gesture, Gesture::On { .. }))
        .collect();
    assert_eq!(triggers.len(), 3);
    for trigger in triggers {
        verify_trigger(&notes, trigger.input);
    }
    assert!(notes.iter().any(|note| {
        note.expression
            .points
            .iter()
            .any(|p| p.value.pitch > 200_000)
    }));
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/firmware-native/humanized-{model}.json"
    ));
    r.command(BridgeCommand::Save {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    r.command(BridgeCommand::Open {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(r.host.engine.session(), &session);
    let mut replay = Engine::new(API_VERSION, session.clone(), Limits::default()).unwrap();
    replay
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    replay.advance(BeatTime(clip.length.0 - 1)).unwrap();
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
fn verify_trigger(
    notes: &[crest_performance_core::Note],
    input: crest_performance_core::InputEvent,
) {
    let Gesture::On { key, velocity, .. } = input.gesture else {
        panic!()
    };
    let pitches = match key {
        43 => [71, 74, 77],
        53 => [64, 67, 71],
        63 => [69, 72, 76],
        _ => panic!("unexpected raw firmware pitch"),
    };
    assert_eq!(input.note.source.0, 1);
    assert_eq!(input.note.track.0, 1);
    // Fixed independent vectors: seed 42, source 1, track 1, trigger IDs 1–3.
    let timing = match input.note.note.0 {
        1 => [(49021, 5), (35462, 3), (32129, 1)],
        2 => [(55456, 8), (32423, 1), (1415, 4)],
        3 => [(32528, -4), (22975, -6), (24336, -2)],
        _ => panic!("unexpected trigger identity"),
    };
    for (key, (delay, variation)) in pitches.into_iter().zip(timing) {
        let note = notes
            .iter()
            .find(|note| note.key == key && note.start.0 == input.at.0 + delay)
            .unwrap();
        assert_eq!(
            note.velocity,
            (i16::from(velocity) + variation).clamp(1, 127) as u8
        );
    }
}
