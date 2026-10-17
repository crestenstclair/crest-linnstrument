use crest_linnstrument::*;

fn cc(number: u8, value: u8) -> MidiMessage {
    MidiMessage::channel(0xb0, 2, number, value)
}
#[test]
fn row_expression_tracks_firmware_slide_and_ignores_other_fingers_and_partial_pairs() {
    for geometry in [Geometry::Pads128, Geometry::Pads200] {
        let mut row = UserRowX::new(2, geometry).unwrap();
        assert_eq!(row.accept(cc(1, 10)).unwrap(), None);
        row.accept(MidiMessage::channel(0x90, 2, 1, 100)).unwrap();
        row.accept(cc(33, 0)).unwrap();
        assert_eq!(row.accept(cc(1, 0)).unwrap(), Some(0));
        // Another finger cannot overwrite the first finger's position.
        row.accept(MidiMessage::channel(0x90, 2, 8, 100)).unwrap();
        row.accept(cc(40, 127)).unwrap();
        assert_eq!(row.accept(cc(8, 20)).unwrap(), None);
        row.accept(cc(119, 1)).unwrap();
        row.accept(MidiMessage::channel(0x90, 2, 2, 100)).unwrap();
        row.accept(MidiMessage::channel(0x80, 2, 1, 2)).unwrap();
        let maximum = if geometry == Geometry::Pads200 {
            4265
        } else {
            2730
        };
        row.accept(cc(34, (maximum & 127) as u8)).unwrap();
        assert_eq!(
            row.accept(cc(2, (maximum >> 7) as u8)).unwrap(),
            Some(65_535)
        );
        assert_eq!(row.accept(cc(2, 1)).unwrap(), None, "no stale LSB reuse");
        row.accept(MidiMessage::channel(0x80, 2, 2, 0)).unwrap();
        row.accept(cc(40, 10)).unwrap();
        assert_eq!(
            row.accept(cc(8, 5)).unwrap(),
            None,
            "other held contact must release and press again"
        );
        row.reset();
        assert_eq!(row.accept(cc(2, 0)).unwrap(), None);
    }
}
#[test]
fn unrelated_messages_cancel_incomplete_slide_handover() {
    let mut row = UserRowX::new(2, Geometry::Pads200).unwrap();
    row.accept(MidiMessage::channel(0x90, 2, 1, 100)).unwrap();
    row.accept(cc(119, 1)).unwrap();
    row.accept(cc(33, 0)).unwrap();
    row.accept(MidiMessage::channel(0x90, 2, 2, 100)).unwrap();
    row.accept(cc(34, 0)).unwrap();
    assert_eq!(row.accept(cc(2, 1)).unwrap(), None);
    row.accept(cc(33, 0)).unwrap();
    assert_eq!(row.accept(cc(1, 0)).unwrap(), Some(0));
}
