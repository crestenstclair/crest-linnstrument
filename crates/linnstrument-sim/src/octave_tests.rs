use super::*;
use crate::controller_hold_tests::{assert_color, attacks, contact};

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_octave_holds_shift_only_selected_clip_and_restore_on_release() {
    for model in [128, 200] {
        verify_octaves(model);
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
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    contact(&mut r, 1, 1.0, 0.0);
    contact(&mut r, 2, 2.0, 0.0);
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    r
}
fn assert_attacks(r: &mut Runtime, shifted: u8) {
    let attacks = attacks(r);
    assert_eq!(
        attacks.iter().filter(|key| **key == shifted).count(),
        4,
        "{attacks:?}"
    );
    assert_eq!(
        attacks.iter().filter(|key| **key == 67).count(),
        4,
        "{attacks:?}"
    );
    assert_eq!(attacks.len(), 8);
}
fn ui_hold(r: &mut Runtime, direction: OctaveDirection, held: bool) {
    r.command(BridgeCommand::ClipOctave {
        slot: 0,
        direction,
        held,
    })
    .unwrap();
}
fn verify_octaves(model: u16) {
    let mut r = prepare(model);
    let saved = r.host.document().unwrap();
    assert_attacks(&mut r, 60);
    contact(&mut r, 10, 1.0, 4.0);
    assert_eq!(r.host.engine.snapshot().clip_transpositions[..2], [12, 0]);
    assert_attacks(&mut r, 72);
    assert_color(&r, 12, [true, false, true]);
    ui_hold(&mut r, OctaveDirection::Up, true);
    r.command(BridgeCommand::Release { id: 10 }).unwrap();
    r.advance(100_000).unwrap();
    assert_attacks(&mut r, 72);
    contact(&mut r, 11, 1.0, 3.0);
    assert_eq!(r.host.engine.snapshot().clip_transpositions[0], 0);
    assert_attacks(&mut r, 60);
    ui_hold(&mut r, OctaveDirection::Up, false);
    r.advance(100_000).unwrap();
    assert_attacks(&mut r, 48);
    assert_color(&r, 11, [true, false, true]);
    r.command(BridgeCommand::ReleaseMomentary {}).unwrap();
    r.advance(100_000).unwrap();
    assert_attacks(&mut r, 60);
    assert_color(&r, 11, [false, false, true]);
    assert_color(&r, 12, [false, false, true]);
    assert_eq!(r.host.document().unwrap(), saved);
    r.command(BridgeCommand::Stop {}).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    assert!(
        r.host
            .engine
            .snapshot()
            .clip_transpositions
            .iter()
            .all(|shift| *shift == 0)
    );
    r.command(BridgeCommand::Mode { clips: false }).unwrap();
    r.command(BridgeCommand::PlayExample {}).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
}
