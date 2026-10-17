use crest_linnstrument::*;
use crest_performance_core::*;

fn send(input: &mut NoteInput, status: u8, channel: u8, a: u8, b: u8) -> Vec<InputEvent> {
    input
        .accept(BeatTime(123), MidiMessage::channel(status, channel, a, b))
        .unwrap()
        .to_vec()
}
fn expression(event: InputEvent) -> Expression {
    match event.gesture {
        Gesture::On { expression, .. } | Gesture::Expression { expression } => expression,
        _ => panic!("expected expression"),
    }
}
#[test]
fn sustain_reuses_input_member_without_reassigning_old_note_expression() {
    let mut input = NoteInput::mpe(MpeZone::lower(), SourceId(1), TrackId(1));
    let a = send(&mut input, 0x90, 1, 60, 90)[0].note;
    let b = send(&mut input, 0x90, 2, 64, 90)[0].note;
    send(&mut input, 0xb0, 0, 64, 127);
    assert!(send(&mut input, 0x80, 1, 60, 42).is_empty());
    assert!(send(&mut input, 0x80, 2, 64, 43).is_empty());
    let c = send(&mut input, 0x90, 1, 67, 90)[0].note;
    assert_ne!(a, c);
    let bent = send(&mut input, 0xe0, 1, 0, 65);
    assert_eq!(bent.len(), 1);
    assert_eq!(bent[0].note, c);
    assert_eq!(expression(bent[0]).pitch, 750_000);
    let releases = send(&mut input, 0xb0, 0, 64, 0);
    assert_eq!(releases.iter().map(|e| e.note).collect::<Vec<_>>(), [a, b]);
    assert_eq!(releases[0].gesture, Gesture::Off { velocity: 42 });
    assert_eq!(input.active_notes(), 1);
    assert_eq!(send(&mut input, 0x80, 1, 67, 0)[0].note, c);
}
#[test]
fn normal_polyphony_channel_expression_key_pressure_and_fifo_releases() {
    let mut input = NoteInput::polyphonic(Some(0), 2, SourceId(1), TrackId(1)).unwrap();
    let a = send(&mut input, 0x90, 0, 60, 90)[0].note;
    let b = send(&mut input, 0x90, 0, 64, 90)[0].note;
    let c = send(&mut input, 0x90, 0, 60, 90)[0].note;
    assert!(send(&mut input, 0x90, 1, 72, 90).is_empty());
    let bent = send(&mut input, 0xe0, 0, 0, 96);
    assert_eq!(bent.len(), 3);
    assert!(bent.iter().all(|e| expression(*e).pitch == 1_000_000));
    let pressure = send(&mut input, 0xa0, 0, 64, 127);
    assert_eq!(pressure.len(), 1);
    assert_eq!(pressure[0].note, b);
    let moved = send(&mut input, 0xb0, 0, 74, 100);
    assert_eq!(expression(moved[1]).pressure, 65535);
    assert_eq!(expression(moved[0]).pressure, 0);
    assert_eq!(send(&mut input, 0x80, 0, 60, 0)[0].note, a);
    assert_eq!(send(&mut input, 0x80, 0, 60, 0)[0].note, c);
    assert_eq!(send(&mut input, 0xb0, 0, 120, 0)[0].note, b);
    assert_eq!(input.active_notes(), 0);
}
#[test]
fn bend_rpn_and_zone_changes_validate_before_altering_live_ownership() {
    let mut input = NoteInput::mpe(MpeZone::lower(), SourceId(1), TrackId(1));
    send(&mut input, 0xb0, 0, 101, 0);
    send(&mut input, 0xb0, 0, 100, 6);
    send(&mut input, 0xb0, 0, 6, 3);
    assert!(send(&mut input, 0x90, 4, 72, 90).is_empty());
    send(&mut input, 0xb0, 1, 101, 0);
    send(&mut input, 0xb0, 1, 100, 0);
    send(&mut input, 0xb0, 1, 6, 12);
    send(&mut input, 0xb0, 1, 38, 50);
    assert_eq!(input.bend_range_millionths(1).unwrap(), 12_500_000);
    send(&mut input, 0xe0, 1, 0, 96);
    let start = send(&mut input, 0x90, 1, 60, 90)[0];
    assert_eq!(expression(start).pitch, 6_250_000);
    assert!(
        input
            .accept(BeatTime(124), MidiMessage::channel(0xb0, 1, 6, 48))
            .is_err()
    );
    assert_eq!(input.bend_range_millionths(1).unwrap(), 12_500_000);
    assert!(
        input
            .accept(BeatTime(124), MidiMessage::channel(0xb0, 0, 6, 4))
            .is_err()
    );
    send(&mut input, 0x80, 1, 60, 0);
    send(&mut input, 0xb0, 1, 6, 48);
    send(&mut input, 0xb0, 1, 38, 0);
    assert_eq!(
        expression(send(&mut input, 0x90, 1, 60, 90)[0]).pitch,
        24_000_000
    );
    send(&mut input, 0xb0, 1, 99, 1); // NRPN must cancel RPN selection.
    send(&mut input, 0xb0, 1, 6, 10);
    assert_eq!(input.bend_range_millionths(1).unwrap(), 48_000_000);
}
#[test]
fn main_bend_fans_out_and_capacity_never_loses_release() {
    let mut input = NoteInput::mpe(
        MpeZone::new(15, 12, 14, 48).unwrap(),
        SourceId(1),
        TrackId(1),
    );
    let a = send(&mut input, 0x90, 12, 60, 90)[0].note;
    let b = send(&mut input, 0x90, 13, 64, 90)[0].note;
    let moved = send(&mut input, 0xe0, 15, 0, 96);
    assert_eq!(moved.iter().map(|e| e.note).collect::<Vec<_>>(), [a, b]);
    assert!(moved.iter().all(|e| expression(*e).pitch == 1_000_000));
    assert_eq!(send(&mut input, 0xb0, 15, 123, 0).len(), 2);
    let mut input = NoteInput::polyphonic(Some(0), 2, SourceId(1), TrackId(1)).unwrap();
    for key in 0..128 {
        send(&mut input, 0x90, 0, key, 90);
    }
    assert!(
        input
            .accept(BeatTime(123), MidiMessage::channel(0x90, 0, 60, 90))
            .is_err()
    );
    let releases = send(&mut input, 0xb0, 0, 120, 0);
    assert_eq!(releases.len(), 128);
    assert_eq!(input.active_notes(), 0);
}
