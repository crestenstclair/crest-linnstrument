use crest_linnstrument::Geometry;
use crest_performance_core::*;
use crest_performance_host::{TestHost, storage};
#[test]
fn parameter_capture_and_note_expression_remain_separate_through_reopen() {
    let scene = vec![MorphParameter {
        address: ParameterAddress {
            destination: DestinationId(1),
            parameter: 9,
        },
        a: 100,
        b: 60_000,
    }]
    .try_into()
    .unwrap();
    let mut host = TestHost::new(Session::default(), Geometry::Pads200).unwrap();
    host.set_scene_morph(Some(scene)).unwrap();
    for byte in [0x91, 60, 100] {
        host.accept_bytes(1000, byte, Geometry::Pads200).unwrap();
    }
    let note_before = host.capture.clone();
    host.command(Command::SetMorphPosition(65_535)).unwrap();
    assert_eq!(host.capture, note_before);
    assert_eq!(host.parameter_capture.last().unwrap().value, 60_000);
    assert_eq!(host.engine.snapshot().active_voices, 1);
    let saved = host.document().unwrap();
    let reopened: storage::HostDocument =
        serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    let mut host = TestHost::from_document(reopened, Geometry::Pads128).unwrap();
    assert!(host.parameter_capture.is_empty());
    assert_eq!(host.engine.snapshot().morph_position, None);
    host.command(Command::SetMorphPosition(0)).unwrap();
    assert_eq!(host.parameter_capture[0].value, 100);
}
