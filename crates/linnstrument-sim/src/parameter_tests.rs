use super::*;
use crest_performance_core::SlotState;

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_phrase_note_parameters_survive_reopen_and_pad_launched_loops() {
    for model in [128, 200] {
        verify_locks(model);
    }
}
fn verify_locks(model: u16) {
    let mut r = super::harmony_tests::runtime(model);
    let original = record_and_reopen(&mut r, model);
    let notes = original.notes().unwrap();
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    r.advance(50_000).unwrap();
    pad(&mut r, 1);
    let launch = r.host.engine.slot_launch_time(0).unwrap().unwrap();
    let end = launch.0 + 3 * original.length.0 - original.length.0 / 10;
    while r.host.engine.position().0 < end {
        r.advance(10_000).unwrap();
    }
    let mut expected = Vec::new();
    for iteration in 0..3 {
        append_iteration(
            &mut expected,
            launch.0 + iteration * original.length.0,
            &notes,
        );
    }
    assert_eq!(
        r.host
            .parameter_capture
            .iter()
            .map(|event| (event.at.0, event.value))
            .collect::<Vec<_>>(),
        expected
    );
    assert!(
        r.host
            .parameter_capture
            .iter()
            .all(|event| event.address.parameter == 0 && event.voice.is_some())
    );
    let voices: std::collections::BTreeSet<_> = r
        .host
        .parameter_capture
        .iter()
        .map(|event| event.voice.unwrap())
        .collect();
    assert_eq!(voices.len(), 9);
    let [red, green, blue] = r.firmware.leds[8];
    assert!(green > 200 && red < 50 && blue < 50);
    pad(&mut r, 2);
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Recorded);
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    let [red, green, blue] = r.firmware.leds[8];
    assert!(blue > 200 && red < 50 && green < 50);
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Play,
    })
    .unwrap();
    let count = r.host.parameter_capture.len();
    r.command(BridgeCommand::PlayExample {}).unwrap();
    assert_eq!(
        r.host.parameter_capture.len(),
        count,
        "ordinary live notes inherit no clip locks"
    );
}
fn record_and_reopen(r: &mut Runtime, model: u16) -> crest_performance_core::Clip {
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    r.command(BridgeCommand::PlayExample {}).unwrap();
    r.advance(1_000_000).unwrap();
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    let original = r.host.engine.session().slots[0].clip.clone().unwrap();
    let notes = original.notes().unwrap();
    assert_eq!(notes.len(), 3);
    for (note, value) in notes.iter().zip([0, 32_768, 65_535]) {
        r.command(BridgeCommand::NoteParameter {
            slot: 0,
            note: note.id,
            parameter: 0,
            value: Some(value),
        })
        .unwrap();
    }
    let saved = r.host.engine.save_session().unwrap();
    assert_eq!(
        saved.slots[0].clip.as_ref().unwrap().events,
        original.events
    );
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/firmware-native/note-parameters-{model}.json"
    ));
    r.command(BridgeCommand::Save {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    r.command(BridgeCommand::Open {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(r.host.engine.session(), &saved);
    assert!(r.host.parameter_capture.is_empty());
    original
}

fn append_iteration(
    expected: &mut Vec<(u64, u16)>,
    start: u64,
    notes: &[crest_performance_core::Note],
) {
    expected.extend(
        notes
            .iter()
            .zip([0, 32_768, 65_535])
            .map(|(note, value)| (start + note.start.0, value)),
    );
}
fn pad(r: &mut Runtime, id: u32) {
    r.set_contact(Contact {
        id,
        x: 1.0,
        y: 0.0,
        z: 0.5,
        latched: false,
    })
    .unwrap();
    r.advance(100_000).unwrap();
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(50_000).unwrap();
}
