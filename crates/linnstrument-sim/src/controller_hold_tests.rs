use super::*;
use crest_performance_core::{NoteCondition, SlotState};

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_pads_hold_fill_and_mute_independently_and_recover_on_focus_loss() {
    for model in [128, 200] {
        verify_holds(model);
    }
}

fn prepare(model: u16) -> Runtime {
    let mut r = super::harmony_tests::runtime(model);
    for slot in 0..2 {
        r.command(BridgeCommand::Harmony {
            settings: HarmonySettings {
                degree: if slot == 0 { 0 } else { 4 },
                ..HarmonySettings::default()
            },
        })
        .unwrap();
        r.command(BridgeCommand::Generate {
            slot,
            settings: PatternSettings {
                steps: 4,
                pulses: 4,
                step: BeatTime(96_000),
                gate: BeatTime(48_000),
                tone_min: 0,
                tone_max: 0,
                ..PatternSettings::default()
            },
        })
        .unwrap();
    }
    let notes = r.host.engine.session().slots[1]
        .clip
        .as_ref()
        .unwrap()
        .notes()
        .unwrap();
    for note in notes {
        r.command(BridgeCommand::NoteCondition {
            slot: 1,
            note: note.id,
            condition: NoteCondition::Fill {},
        })
        .unwrap();
    }
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    contact(&mut r, 1, 1.0, 0.0);
    contact(&mut r, 2, 2.0, 0.0);
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    r
}
pub(super) fn contact(r: &mut Runtime, id: u32, x: f64, y: f64) {
    r.set_contact(Contact {
        id,
        x,
        y,
        z: 0.55,
        latched: false,
    })
    .unwrap();
    r.advance(80_000).unwrap();
    // The preceding native step sends host LED intent; firmware must then
    // parse those bytes and complete the decoder's >=25 ms exposure window.
    r.advance(50_000).unwrap();
}
pub(super) fn attacks(r: &mut Runtime) -> Vec<u8> {
    let before = r.host.output_count();
    r.advance(200_000).unwrap();
    r.host
        .output_since(before)
        .unwrap()
        .iter()
        .filter(|event| event.message.bytes()[0] & 0xf0 == 0x90)
        .map(|event| event.message.bytes()[1])
        .collect()
}
pub(super) fn assert_color(r: &Runtime, cell: usize, expected: [bool; 3]) {
    for (actual, on) in r.firmware.leds[cell].into_iter().zip(expected) {
        assert!(
            if on { actual > 200 } else { actual < 50 },
            "LED {cell}: {:?}, expected {expected:?}",
            r.firmware.leds[cell]
        );
    }
}
fn verify_holds(model: u16) {
    let mut r = prepare(model);
    let saved = r.host.document().unwrap();
    assert_eq!(
        &r.host.engine.snapshot().slots[..2],
        &[SlotState::Playing; 2]
    );
    assert_eq!(attacks(&mut r), [60; 4]);
    assert_color(&r, 14, [true, true, false]); // Fill ready: yellow
    contact(&mut r, 10, 1.0, 6.0);
    assert!(r.host.engine.snapshot().fill);
    assert_color(&r, 14, [true, false, true]); // Fill held: magenta
    contact(&mut r, 11, 1.0, 1.0);
    assert!(r.host.engine.snapshot().muted_slots[0]);
    assert_eq!(attacks(&mut r), [67; 4]);
    assert_color(&r, 8, [false, true, true]);
    assert_color(&r, 9, [false, true, true]);
    r.command(BridgeCommand::Release { id: 10 }).unwrap();
    r.advance(80_000).unwrap();
    assert!(!r.host.engine.snapshot().fill);
    assert!(r.host.engine.snapshot().muted_slots[0]);
    assert!(attacks(&mut r).is_empty());
    r.command(BridgeCommand::Release { id: 11 }).unwrap();
    r.advance(80_000).unwrap();
    assert!(!r.host.engine.snapshot().muted_slots[0]);
    assert_eq!(attacks(&mut r), [60; 4]);
    assert_color(&r, 8, [false, true, false]);
    assert_color(&r, 9, [false, false, true]);
    contact(&mut r, 12, 1.0, 6.0);
    contact(&mut r, 13, 1.0, 1.0);
    r.command(BridgeCommand::ReleaseMomentary {}).unwrap();
    r.advance(100_000).unwrap();
    assert!(r.contacts.is_empty());
    assert!(!r.host.engine.snapshot().fill);
    assert!(!r.host.engine.snapshot().muted_slots[0]);
    assert_eq!(attacks(&mut r), [60; 4]);
    assert_eq!(r.host.document().unwrap(), saved);
    r.command(BridgeCommand::Stop {}).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
}
