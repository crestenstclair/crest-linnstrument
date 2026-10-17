use super::*;
use crest_performance_core::{ExpressionPolicy, Gesture, HarmonyPlaying, SlotState};

fn runtime(model: u16) -> Runtime {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let flash = root.join(format!("target/firmware-native/ideal-{model}.bin"));
    let (worker, firmware) = Worker::start(
        &root.join("target/firmware-native/linnstrument-worker"),
        &flash,
        model,
    )
    .unwrap();
    let geometry = if model == 200 {
        Geometry::Pads200
    } else {
        Geometry::Pads128
    };
    let mut runtime = Runtime {
        run: GestureRun::new(&firmware, &flash).unwrap(),
        worker,
        firmware,
        host: TestHost::new(Session::default(), geometry).unwrap(),
        geometry,
        contacts: vec![],
        scenario: None,
        recent_midi: std::collections::VecDeque::new(),
        verified_steps: None,
        paused: true,
        connected: true,
        #[cfg(feature = "midi")]
        midi: None,
    };
    runtime.advance(1_000_000).unwrap();
    runtime
}
#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_chord_phrase_preserves_three_gestures_and_launches_rendered_clip() {
    for model in [128, 200] {
        verify_chord_phrase(model);
    }
}
fn verify_chord_phrase(model: u16) {
    let mut r = runtime(model);
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    r.command(BridgeCommand::PlayExample {}).unwrap();
    r.advance(250_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 9);
    assert!(r.firmware.sensor_reads > 1000);
    r.advance(750_000).unwrap();
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    let session = r.host.engine.save_session().unwrap();
    let clip = session.slots[0].clip.as_ref().unwrap();
    let notes = clip.notes().unwrap();
    assert_eq!(notes.len(), 9);
    let mut played: Vec<_> = notes.iter().map(|n| n.key).collect();
    played.sort_unstable();
    assert_eq!(played, [64, 67, 69, 71, 71, 72, 74, 76, 77]);
    let mut inputs: Vec<_> = clip
        .harmony_origin
        .iter()
        .filter_map(|g| {
            if let Gesture::On { key, .. } = g.input.gesture {
                Some(key)
            } else {
                None
            }
        })
        .collect();
    inputs.sort_unstable();
    assert_eq!(
        inputs,
        [43, 53, 63],
        "the firmware played three notes; library rendered nine voices"
    );
    for chord in notes.chunks_exact(3) {
        assert_eq!(chord[0].expression, chord[1].expression);
        assert_eq!(chord[0].expression, chord[2].expression);
    }
    let bent = notes.chunks_exact(3).find(|c| c[0].key == 71).unwrap();
    let vertical = notes.chunks_exact(3).find(|c| c[0].key == 64).unwrap();
    let pressure = notes.chunks_exact(3).find(|c| c[0].key == 69).unwrap();
    assert!(
        bent[0]
            .expression
            .points
            .iter()
            .any(|p| p.value.pitch > 200_000)
    );
    assert!(
        vertical[0]
            .expression
            .points
            .iter()
            .all(|p| p.value.pitch.abs() < 100_000)
    );
    assert!(
        pressure[0]
            .expression
            .points
            .iter()
            .all(|p| p.value.pitch.abs() < 100_000)
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(format!("target/firmware-native/harmony-{model}.json"));
    crest_performance_host::storage::save_session(&path, &session).unwrap();
    r.command(BridgeCommand::Open {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(r.host.engine.session(), &session);
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    r.set_contact(Contact {
        id: 99,
        x: 1.0,
        y: 0.0,
        z: 0.5,
        latched: false,
    })
    .unwrap();
    r.advance(200_000).unwrap();
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Playing);
    assert!(r.host.engine.snapshot().active_voices >= 3);
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    let led = r.firmware.leds[8];
    assert!(
        led[1] > led[0] && led[1] > led[2],
        "playing LED must come from firmware: {led:?}"
    );
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    r.host.disconnect().unwrap();
}
#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_strumplate_and_top_voice_policy_use_stock_mpe() {
    let mut r = runtime(200);
    r.command(BridgeCommand::Harmony {
        settings: HarmonySettings {
            playing: HarmonyPlaying::Strumplate,
            expression: ExpressionPolicy::TopVoice,
            ..HarmonySettings::default()
        },
    })
    .unwrap();
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    r.set_contact(Contact {
        id: 1,
        x: 1.0,
        y: 0.0,
        z: 0.5,
        latched: false,
    })
    .unwrap();
    r.advance(200_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 1);
    assert!(
        r.host
            .capture
            .iter()
            .any(|m| m.message.bytes()[0] & 0xf0 == 0x90 && m.message.bytes()[1] == 60)
    );
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
}
