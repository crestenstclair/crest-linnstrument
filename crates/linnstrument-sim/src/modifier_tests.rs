use super::*;
use crest_performance_core::{ChordExtension, LearnedChord, Suspension};

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_modifier_pads_record_exact_chord_tones_and_reopen_with_actual_led_feedback() {
    for model in [128, 200] {
        verify_modifiers(model);
        verify_learned_modifiers_are_dark(model);
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
    r.advance(80_000).unwrap();
}
fn record_chord(r: &mut Runtime, expected: &[u8]) {
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    touch(r, 1.0, 0.0);
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    let clip = r.host.engine.session().slots[0].clip.as_ref().unwrap();
    let notes: Vec<_> = clip.notes().unwrap().iter().map(|note| note.key).collect();
    assert_eq!(notes, expected);
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    assert_eq!(
        clip.harmony_origin[0].settings,
        r.host.engine.snapshot().harmony
    );
}
fn assert_led(r: &Runtime, column: usize, enabled: bool) {
    let [red, green, blue] = r.firmware.leds[column * 8 + 5];
    assert!(red < 50);
    if enabled {
        assert!(green > 200 && blue < 50, "modifier {column} enabled");
    } else {
        assert!(blue > 200 && green < 50, "modifier {column} ready");
    }
}
fn verify_modifiers(model: u16) {
    let mut r = super::harmony_tests::runtime(model);
    let reads = r.firmware.sensor_reads;
    let cases: [(u8, &[u8]); 10] = [
        (1, &[60, 64, 67, 71]),
        (2, &[60, 64, 67, 71, 74]),
        (1, &[60, 64, 67, 71]),
        (1, &[60, 64, 67]),
        (3, &[60, 62, 67]),
        (4, &[60, 65, 67]),
        (4, &[60, 64, 67]),
        (5, &[60, 63, 67]),
        (1, &[60, 63, 67, 70]),
        (2, &[60, 63, 67, 70, 74]),
    ];
    for (column, expected) in cases {
        r.command(BridgeCommand::Mode { clips: true }).unwrap();
        touch(&mut r, f64::from(column), 5.0);
        let settings = r.host.engine.snapshot().harmony;
        assert_led(&r, 1, settings.extension == ChordExtension::Seventh);
        assert_led(&r, 2, settings.extension == ChordExtension::Ninth);
        assert_led(&r, 3, settings.suspension == Suspension::Second);
        assert_led(&r, 4, settings.suspension == Suspension::Fourth);
        assert_led(&r, 5, settings.borrowed);
        record_chord(&mut r, expected);
    }
    assert!(r.firmware.sensor_reads > reads + 1000);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/firmware-native/modifiers-{model}.json"
    ));
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
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    touch(&mut r, 1.0, 0.0);
    assert_eq!(
        r.host.engine.snapshot().slots[0],
        crest_performance_core::SlotState::Playing
    );
    assert_led(&r, 2, true);
    assert_led(&r, 5, true);
    r.command(BridgeCommand::Stop {}).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
}
fn verify_learned_modifiers_are_dark(model: u16) {
    let mut session = Session::default();
    session.harmony.learned[0] = Some(LearnedChord::new(&[60, 67, 72], 60).unwrap());
    let mut r = super::harmony_tests::runtime_with_session(model, session);
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    let original = r.host.document().unwrap();
    for column in 1..=5 {
        touch(&mut r, column as f64, 5.0);
        assert_eq!(r.host.document().unwrap(), original);
        assert!(r.firmware.leds[column * 8 + 5].iter().max().unwrap() < &50);
    }
    record_chord(&mut r, &[60, 67, 72]);
}
