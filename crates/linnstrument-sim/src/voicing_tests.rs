use super::*;
use crest_performance_core::{Gesture, SlotState};

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_voicing_pads_change_future_notes_preserve_clips_and_render_actual_leds() {
    for model in [128, 200] {
        verify_voicing(model);
    }
}
fn touch(r: &mut Runtime, x: f64, y: f64) {
    r.set_contact(Contact {
        id: 1,
        x,
        y,
        z: 0.55,
        latched: false,
    })
    .unwrap();
    r.advance(100_000).unwrap();
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(80_000).unwrap(); // firmware parses feedback and completes LED exposure
}
fn attacks(r: &Runtime, after: u64) -> Vec<u8> {
    r.host
        .output_since(after)
        .unwrap()
        .iter()
        .filter_map(|event| {
            let bytes = event.message.bytes();
            (bytes[0] & 0xf0 == 0x90).then_some(bytes[1])
        })
        .collect()
}
fn verify_voicing(model: u16) {
    let mut r = super::harmony_tests::runtime(model);
    r.command(BridgeCommand::Generate {
        slot: 0,
        settings: PatternSettings {
            steps: 4,
            pulses: 4,
            step: BeatTime(240_000),
            gate: BeatTime(120_000),
            tone_min: 0,
            tone_max: 0,
            follow_harmony: true,
            ..PatternSettings::default()
        },
    })
    .unwrap();
    let clip = r.host.engine.session().slots[0].clip.clone().unwrap();
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    touch(&mut r, 1.0, 0.0);
    let start = r.host.engine.slot_launch_time(0).unwrap();
    let reads = r.firmware.sensor_reads;
    // Independently specified C-major pitches, not values from the voicing implementation.
    for (column, expected) in [
        (3, 64),
        (2, 60),
        (5, 72),
        (4, 60),
        (6, 60),
        (7, 60),
        (3, 64),
        (5, 76),
    ] {
        touch(&mut r, f64::from(column), 6.0);
        let before = r.host.output_count();
        r.advance(200_000).unwrap();
        let notes = attacks(&r, before);
        assert!(!notes.is_empty());
        assert_eq!(notes, vec![expected; notes.len()], "column {column}");
        assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Playing);
        assert_eq!(r.host.engine.slot_launch_time(0).unwrap(), start);
        assert_eq!(r.host.engine.session().slots[0].clip.as_ref(), Some(&clip));
    }
    assert!(r.firmware.sensor_reads > reads + 1000);
    for column in [6, 7] {
        let [red, green, blue] = r.firmware.leds[column * 8 + 6];
        assert!(
            red < 50 && green > 200 && blue < 50,
            "enabled voicing toggle is green"
        );
    }
    r.command(BridgeCommand::Stop {}).unwrap();
    record_selected_voicing(&mut r, model);
    verify_unavailable_steps(&mut r);
}
fn verify_unavailable_steps(r: &mut Runtime) {
    r.command(BridgeCommand::Harmony {
        settings: HarmonySettings {
            tonic: 120,
            ..HarmonySettings::default()
        },
    })
    .unwrap();
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    r.command(BridgeCommand::HarmonySource {
        source: HarmonySource::Clip { slot: 0 },
    })
    .unwrap();
    let saved = r.host.document().unwrap();
    for column in [3, 5, 6] {
        touch(r, column as f64, 6.0);
        assert_eq!(r.host.document().unwrap(), saved);
        assert!(r.firmware.leds[column * 8 + 6].iter().max().unwrap() < &50);
    }
}
fn record_selected_voicing(r: &mut Runtime, model: u16) {
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    r.command(BridgeCommand::BeginRecord { slot: 1 }).unwrap();
    touch(r, 1.0, 0.0);
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    let clip = r.host.engine.session().slots[1].clip.as_ref().unwrap();
    let notes: Vec<_> = clip
        .events
        .iter()
        .filter_map(|event| match event.gesture {
            Gesture::On { key, .. } => Some(key),
            _ => None,
        })
        .collect();
    assert_eq!(notes, [76, 84, 91]); // C first inversion, spread, register +1
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../target/firmware-native/voicing-{model}.json"));
    r.command(BridgeCommand::Save {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    let saved = r.host.document().unwrap();
    r.command(BridgeCommand::Open {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(r.host.document().unwrap(), saved);
}
