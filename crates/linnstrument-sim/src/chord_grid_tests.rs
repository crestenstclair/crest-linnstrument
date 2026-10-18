use super::*;
use crate::harmony_tests::runtime_with_session;
use crest_performance_core::{ChordExtension, ChordQuality};

fn touch(r: &mut Runtime, id: u32, x: f64, y: f64, z: f64) {
    r.set_contact(Contact {
        id,
        x,
        y,
        z,
        latched: false,
    })
    .unwrap();
    r.advance(100_000).unwrap();
}
fn notes(r: &Runtime) -> Vec<u8> {
    r.host
        .capture
        .iter()
        .filter_map(|m| {
            let [s, k, v] = m.message.bytes();
            (s & 0xf0 == 0x90 && v > 0).then_some(k)
        })
        .collect()
}
#[test]
#[ignore = "requires native firmware worker"]
fn dedicated_chord_grid_uses_firmware_sensors_leds_and_independent_expression() {
    for model in [128, 200] {
        verify_grid(model);
    }
}
fn verify_grid(model: u16) {
    let first = if model == 128 { 10 } else { 14 };
    let (minor, add9) = if model == 128 {
        (2.0, 8.0)
    } else {
        (6.0, 12.0)
    };
    let mut session = Session::default();
    session.harmony.input_octaves = true;
    session.harmony.input_anchor = 28;
    let mut r = runtime_with_session(model, session);
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    for _ in 0..20 {
        r.advance(20_000).unwrap();
    }
    assert!(r.firmware.surface.user_mode);
    assert_eq!(
        r.firmware.leds[(first - 1) * 8 + 4],
        [0, 0, 0],
        "unassigned gap must be dark at hardware output"
    );
    assert_eq!(r.firmware.leds[6 * 8 + 3], [0, 0, 0]);
    assert!(
        r.firmware.leds[first * 8 + 4][1] > 100,
        "assigned degree has green firmware LED"
    );
    touch(&mut r, 1, first as f64, 4.0, 0.5);
    touch(&mut r, 2, (first + 2) as f64, 4.0, 0.5);
    touch(&mut r, 3, (first + 4) as f64, 4.0, 0.5);
    assert_eq!(notes(&r), [48, 52, 55, 52, 55, 59, 55, 59, 62]);
    let before = r.host.capture.len();
    touch(&mut r, 1, first as f64 + 0.2, 4.0, 0.5);
    assert_dimension(&r, before, 0xe0, &[1, 2, 3]);
    let before = r.host.capture.len();
    touch(&mut r, 2, (first + 2) as f64, 4.2, 0.5);
    assert_dimension(&r, before, 0xb0, &[4, 5, 6]);
    let before = r.host.capture.len();
    touch(&mut r, 3, (first + 4) as f64, 4.0, 0.8);
    assert_dimension(&r, before, 0xd0, &[7, 8, 9]);
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    touch(&mut r, 4, minor, 7.0, 0.5); // minor quality
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(50_000).unwrap();
    touch(&mut r, 5, add9, 7.0, 0.5); // add9
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(50_000).unwrap();
    assert_eq!(
        r.host.engine.snapshot().harmony.quality,
        ChordQuality::Minor
    );
    assert_eq!(
        r.host.engine.snapshot().harmony.extension,
        ChordExtension::AddNinth
    );
    touch(&mut r, 6, first as f64, 5.0, 0.5);
    assert_eq!(&notes(&r)[9..], &[60, 63, 67, 74]);
    touch(&mut r, 6, (first + 1) as f64, 5.0, 0.5);
    assert_eq!(
        r.host.engine.snapshot().active_voices,
        4,
        "crossing cells replaces the chord without stuck voices"
    );
    let pitches = notes(&r);
    assert_eq!(&pitches[pitches.len() - 4..], &[62, 65, 69, 76]);
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Play,
    })
    .unwrap();
    assert!(!r.firmware.surface.user_mode);
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
}
fn assert_dimension(r: &Runtime, before: usize, kind: u8, channels: &[u8]) {
    let changes: Vec<_> = r.host.capture[before..]
        .iter()
        .map(|m| m.message.bytes())
        .filter(|b| b[0] & 0xf0 == kind && (kind != 0xb0 || b[1] == 74))
        .collect();
    assert!(!changes.is_empty(), "firmware expression must reach output");
    assert!(
        changes.iter().all(|b| channels.contains(&(b[0] & 15))),
        "expression leaked: {changes:?}"
    );
}
