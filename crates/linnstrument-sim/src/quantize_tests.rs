use super::*;
use crest_performance_core::SlotState;

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_phrase_quantizes_explicitly_with_lossless_curves_undo_and_pad_playback() {
    for model in [128, 200] {
        verify_quantize(model);
    }
}
fn verify_quantize(model: u16) {
    let mut r = super::harmony_tests::runtime(model);
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    r.advance(70_000).unwrap(); // deliberate off-grid pickup, closer to 1/4 beat
    r.command(BridgeCommand::PlayExample {}).unwrap();
    r.advance(1_000_000).unwrap();
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    let original = r.host.engine.session().slots[0].clip.clone().unwrap();
    let command = BridgeCommand::QuantizeClip {
        slot: 0,
        ticks: 240_000,
    };
    r.command(command).unwrap();
    let changed = r.host.engine.session().slots[0].clip.clone().unwrap();
    let before = original.notes().unwrap();
    let after = changed.notes().unwrap();
    assert_eq!(before.len(), 3);
    assert!(
        before
            .iter()
            .all(|note| note.start.0 > 120_000 && note.start.0 < 240_000)
    );
    assert!(after.iter().all(|note| note.start == BeatTime(240_000)));
    for note in &before {
        let mut quantized = after
            .iter()
            .find(|other| other.id == note.id)
            .unwrap()
            .clone();
        quantized.start = note.start;
        assert_eq!(
            &quantized, note,
            "duration and full X/Y/Z curve are unchanged"
        );
    }
    r.command(BridgeCommand::Undo {}).unwrap();
    assert_eq!(
        r.host.engine.session().slots[0].clip.as_ref(),
        Some(&original)
    );
    r.command(BridgeCommand::QuantizeClip {
        slot: 0,
        ticks: 240_000,
    })
    .unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/firmware-native/quantized-{model}.json"
    ));
    r.command(BridgeCommand::Save {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    r.command(BridgeCommand::Open {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(
        r.host.engine.session().slots[0].clip.as_ref(),
        Some(&changed)
    );
    launch_and_verify(&mut r);
}
fn launch_and_verify(r: &mut Runtime) {
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    let before = r.host.output_count();
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
    r.advance(200_000).unwrap();
    let start = r.host.engine.slot_launch_time(0).unwrap().unwrap();
    let notes: Vec<_> = r
        .host
        .output_since(before)
        .unwrap()
        .iter()
        .filter(|event| event.message.bytes()[0] & 0xf0 == 0x90)
        .collect();
    assert_eq!(notes.len(), 3);
    assert!(notes.iter().all(|event| event.at.0 == start.0 + 240_000));
    let mut keys: Vec<_> = notes.iter().map(|event| event.message.bytes()[1]).collect();
    keys.sort();
    assert_eq!(keys, [43, 53, 63]);
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Playing);
    let [red, green, blue] = r.firmware.leds[8];
    assert!(red < 50 && green > 200 && blue < 50);
    r.command(BridgeCommand::Stop {}).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
}
