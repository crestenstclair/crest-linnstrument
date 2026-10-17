use super::*;
use crest_performance_core::{Gesture, NoteCondition, SlotState};

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_phrase_conditions_launch_from_pad_and_keep_actual_led_feedback() {
    for model in [128, 200] {
        verify_conditions(model);
    }
}

fn verify_conditions(model: u16) {
    let mut r = super::harmony_tests::runtime(model);
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    r.command(BridgeCommand::PlayExample {}).unwrap();
    r.advance(1_000_000).unwrap();
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    let original = r.host.engine.session().slots[0].clip.clone().unwrap();
    let notes = original.notes().unwrap();
    assert_eq!(notes.len(), 3);
    let mut pitches: Vec<_> = notes.iter().map(|n| n.key).collect();
    pitches.sort();
    assert_eq!(pitches, [43, 53, 63]);
    assert!(original.events.iter().any(
        |event| matches!(event.gesture,Gesture::Expression {expression} if expression.pitch>200_000)
    ));
    for (note, condition) in notes.iter().zip([
        NoteCondition::Every {
            period: 4,
            offset: 3,
        },
        NoteCondition::Fill {},
        NoteCondition::NotFill {},
    ]) {
        r.command(BridgeCommand::NoteCondition {
            slot: 0,
            note: note.id,
            condition,
        })
        .unwrap();
    }
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/firmware-native/conditions-{model}.json"
    ));
    r.command(BridgeCommand::Save {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    let session = r.host.engine.save_session().unwrap();
    r.command(BridgeCommand::Open {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(r.host.engine.session(), &session);
    assert_eq!(
        session.slots[0].clip.as_ref().unwrap().events,
        original.events
    );
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    r.advance(50_000).unwrap();
    let captured = r.host.output_count();
    r.set_contact(Contact {
        id: 1,
        x: 1.0,
        y: 0.0,
        z: 0.5,
        latched: false,
    })
    .unwrap();
    r.advance(100_000).unwrap();
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    let launch = r.host.engine.slot_launch_time(0).unwrap().unwrap().0;
    let length = original.length.0;
    advance_until(&mut r, launch + 4 * length - length / 10);
    r.command(BridgeCommand::Fill { held: true }).unwrap();
    advance_until(&mut r, launch + 6 * length - length / 10);
    let actual: Vec<_> = r
        .host
        .output_since(captured)
        .unwrap()
        .iter()
        .filter_map(|event| {
            let bytes = event.message.bytes();
            (bytes[0] & 0xf0 == 0x90).then_some((event.at.0, bytes[1]))
        })
        .collect();
    let mut expected = Vec::new();
    for (iteration, index) in [(0, 2), (1, 2), (2, 2), (3, 0), (3, 2), (4, 1), (5, 1)] {
        expected.push((
            launch + iteration * length + notes[index].start.0,
            notes[index].key,
        ));
    }
    expected.sort();
    assert_eq!(actual, expected);
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Playing);
    let [red, green, blue] = r.firmware.leds[8];
    assert!(
        green > 200 && red < 50 && blue < 50,
        "conditional silence still belongs to a playing clip"
    );
    r.set_contact(Contact {
        id: 2,
        x: 1.0,
        y: 0.0,
        z: 0.5,
        latched: false,
    })
    .unwrap();
    r.advance(100_000).unwrap();
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(50_000).unwrap();
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Recorded);
    let [red, green, blue] = r.firmware.leds[8];
    assert!(blue > 200 && red < 50 && green < 50);
    r.command(BridgeCommand::Stop {}).unwrap();
    assert!(!r.host.engine.snapshot().fill);
}
fn advance_until(r: &mut Runtime, target: u64) {
    while r.host.engine.position().0 < target {
        r.advance(10_000).unwrap();
    }
}
