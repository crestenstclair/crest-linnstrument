use super::*;
use crest_performance_core::{Gesture, SlotState};

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_expression_survives_tempo_changes_record_reopen_and_reset() {
    for model in [128, 200] {
        verify_tempo(model);
    }
}
fn change_tempo(r: &mut Runtime, milli_bpm: u32) {
    let at = r.host.engine.position();
    let voices = r.host.engine.snapshot().active_voices;
    let midi = r.host.output_count();
    r.command(BridgeCommand::Tempo { milli_bpm }).unwrap();
    assert_eq!(r.host.engine.position(), at);
    assert_eq!(r.host.engine.snapshot().active_voices, voices);
    assert_eq!(r.host.output_count(), midi);
}
fn verify_tempo(model: u16) {
    let mut r = super::harmony_tests::runtime(model);
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    let start = r.host.engine.position();
    r.command(BridgeCommand::PlayExample {}).unwrap();
    r.advance(300_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 3);
    let boundary = r.firmware.at_us;
    change_tempo(&mut r, 60_000);
    r.advance(700_000).unwrap();
    let expected = (u128::from(boundary) * 120_000
        + u128::from(r.firmware.at_us - boundary) * 60_000)
        * 960_000
        / 60_000_000_000;
    assert_eq!(r.host.engine.position(), BeatTime(expected as u64));
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    let clip = r.host.engine.session().slots[0].clip.clone().unwrap();
    assert_eq!(clip.length.0, expected as u64 - start.0);
    let mut keys: Vec<_> = clip.notes().unwrap().iter().map(|note| note.key).collect();
    keys.sort();
    assert_eq!(keys, [43, 53, 63]);
    assert!(clip.events.iter().any(|event| matches!(event.gesture,
        Gesture::Expression { expression } if expression.pitch > 200_000)));
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../target/firmware-native/tempo-{model}.json"));
    r.command(BridgeCommand::Save {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    r.set_contact(Contact {
        id: 1,
        x: 1.0,
        y: 0.0,
        z: 0.55,
        latched: false,
    })
    .unwrap();
    r.advance(100_000).unwrap();
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Playing);
    change_tempo(&mut r, 240_000);
    r.advance(200_000).unwrap();
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Playing);
    assert_eq!(r.host.engine.session().slots[0].clip.as_ref(), Some(&clip));
    let [red, green, blue] = r.firmware.leds[8];
    assert!(red < 50 && green > 200 && blue < 50);
    r.command(BridgeCommand::Open {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(r.host.snapshot().tempo_milli_bpm, 60_000);
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    assert_eq!(r.host.engine.session().slots[0].clip.as_ref(), Some(&clip));
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/firmware-native");
    r.reset(
        &root.join("linnstrument-worker"),
        &root.join(format!("ideal-{model}.bin")),
        model,
    )
    .unwrap();
    assert_eq!(r.host.snapshot().tempo_milli_bpm, 60_000);
    assert_eq!(r.host.engine.session().slots[0].clip.as_ref(), Some(&clip));
}
