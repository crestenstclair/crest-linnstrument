use super::*;
use crest_performance_core::{
    DestinationId, MorphParameter, ParameterAddress, ParameterEvent, SceneMorph,
};

fn scene() -> SceneMorph {
    vec![
        MorphParameter {
            address: ParameterAddress {
                destination: DestinationId(1),
                parameter: 1,
            },
            a: 0,
            b: 65_535,
        },
        MorphParameter {
            address: ParameterAddress {
                destination: DestinationId(1),
                parameter: 2,
            },
            a: 65_535,
            b: 0,
        },
    ]
    .try_into()
    .unwrap()
}
fn move_finger(runtime: &mut Runtime, x: f64) {
    runtime
        .command(BridgeCommand::Contact(Contact {
            id: 1,
            x,
            y: 2.0,
            z: 0.55,
            latched: false,
        }))
        .unwrap();
    runtime.advance(60_000).unwrap();
}
#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_scene_row_drives_continuous_capture_and_decoded_leds_in_both_geometries() {
    for model in [128, 200] {
        let first = verify_row(model);
        assert_eq!(
            verify_row(model),
            first,
            "same sensor gestures must produce exact parameter traces"
        );
    }
}
fn verify_row(model: u16) -> Vec<ParameterEvent> {
    let mut r = super::harmony_tests::runtime(model);
    r.command(BridgeCommand::SceneMorph {
        scene: Some(scene()),
    })
    .unwrap();
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    let reads = r.firmware.sensor_reads;
    move_finger(&mut r, 2.0);
    let start = r.host.engine.snapshot().morph_position.unwrap();
    assert!(start > 0 && start < 10_000);
    let initial_count = r.host.parameter_capture.len();
    // Small movements inside one cell prove this is 14-bit firmware X, not a
    // cell click converted into a fabricated application parameter message.
    for step in 1..=6 {
        move_finger(&mut r, 2.0 + f64::from(step) * 0.03);
    }
    assert!(r.host.parameter_capture.len() > initial_count + 2);
    let within = r.host.engine.snapshot().morph_position.unwrap();
    assert!(within > start && within - start < 2000);
    for step in 1..=60 {
        move_finger(&mut r, 2.18 + f64::from(step) * 0.1);
    }
    let end = r.host.engine.snapshot().morph_position.unwrap();
    assert!(end > within + 10_000);
    assert!(r.firmware.sensor_reads > reads + 1000);
    assert!(!r.host.parameter_capture.is_empty());
    for pair in r.host.parameter_capture.chunks_exact(2) {
        assert_eq!(pair[0].at, pair[1].at);
        assert_eq!(u32::from(pair[0].value) + u32::from(pair[1].value), 65_535);
    }
    let previous = end;
    r.command(BridgeCommand::Contact(Contact {
        id: 2,
        x: 12.0,
        y: 2.0,
        z: 0.6,
        latched: false,
    }))
    .unwrap();
    r.advance(100_000).unwrap();
    assert!(
        r.host
            .engine
            .snapshot()
            .morph_position
            .unwrap()
            .abs_diff(previous)
            < 200,
        "second contact cannot steal scene control"
    );
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(200_000).unwrap();
    let position = r.host.engine.snapshot().morph_position.unwrap();
    let span = u32::from(r.geometry.columns() - 2);
    let selected = 1 + (u32::from(position) * span + 32_767) / 65_535;
    let [red, green, blue] = r.firmware.leds[selected as usize * 8 + 2];
    assert!(
        red < 50 && green > 200 && blue < 50,
        "selected position must be decoded firmware green"
    );
    assert!(
        r.host
            .capture
            .iter()
            .all(|event| event.message.bytes()[0] & 0xf0 != 0x90),
        "control row must not create musical notes"
    );
    let trace = r.host.parameter_capture.clone();
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Play,
    })
    .unwrap();
    assert_eq!(r.host.parameter_capture, trace);
    // Stock MPE resumes after leaving User Firmware Mode.
    r.command(BridgeCommand::Contact(Contact {
        id: 1,
        x: 1.0,
        y: 0.0,
        z: 0.55,
        latched: false,
    }))
    .unwrap();
    r.advance(80_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 1);
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(80_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    assert_eq!(r.host.parameter_capture, trace);
    trace
}
