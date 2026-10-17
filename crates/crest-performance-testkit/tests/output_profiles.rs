use crest_linnstrument::*;
use crest_performance_core::*;

fn event(id: u64, gesture: Gesture) -> OutputEvent {
    OutputEvent {
        at: BeatTime(0),
        destination: DestinationId(1),
        track: TrackId(1),
        voice: VoiceId(id),
        gesture,
    }
}
fn on(key: u8, expression: Expression) -> Gesture {
    Gesture::On {
        key,
        velocity: 90,
        expression,
    }
}
fn bytes(batch: &MidiBatch) -> Vec<[u8; 3]> {
    batch.messages().iter().map(|m| m.bytes()).collect()
}
#[test]
fn capabilities_require_explicit_reduction_and_matched_bend_range() {
    let mpe = OutputProfile::default();
    mpe.require_mpe(15, 48).unwrap();
    assert!(mpe.require_mpe(16, 48).is_err());
    assert!(mpe.require_mpe(3, 24).is_err());
    let mono = OutputProfile::Monophonic {
        channel: 0,
        bend_semitones: 48,
    };
    assert!(mono.require_mpe(1, 48).is_err());
    assert_eq!(mono.capabilities().unwrap().sounding_voices, 1);
    assert_eq!(mono.capabilities().unwrap().held_notes, 128);
    assert!(
        MidiOutput::new(
            DestinationId(1),
            OutputProfile::Polyphonic {
                channel: 16,
                voices: 4,
                expression: PolyExpression::PressureOnly
            }
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<OutputProfile>(
            r#"{"type":"monophonic","channel":0,"bend_semitones":48,"ignore_validation":true}"#
        )
        .is_err()
    );
}
#[test]
fn ordinary_polyphony_preserves_per_key_pressure_without_cross_note_controls() {
    let mut output = MidiOutput::new(
        DestinationId(1),
        OutputProfile::Polyphonic {
            channel: 4,
            voices: 3,
            expression: PolyExpression::PressureOnly,
        },
    )
    .unwrap();
    let expressive = Expression {
        pitch: 1_000_000,
        timbre: 55_000,
        pressure: 65_535,
    };
    assert_eq!(
        bytes(&output.accept(event(1, on(60, expressive))).unwrap()),
        [[0x94, 60, 90], [0xa4, 60, 127]]
    );
    output
        .accept(event(2, on(64, Expression::default())))
        .unwrap();
    assert!(
        output
            .accept(event(3, on(60, Expression::default())))
            .is_err()
    );
    assert_eq!(
        bytes(
            &output
                .accept(event(
                    2,
                    Gesture::Expression {
                        expression: expressive
                    }
                ))
                .unwrap()
        ),
        [[0xa4, 64, 127]]
    );
    assert_eq!(
        bytes(
            &output
                .accept(event(1, Gesture::Off { velocity: 17 }))
                .unwrap()
        ),
        [[0x84, 60, 17], [0xa4, 60, 0]]
    );
    let cleanup = output.release_all();
    assert_eq!(cleanup[0].bytes(), [0x84, 64, 0]);
    assert!(!cleanup.iter().any(|m| m.bytes()[0] & 0xf0 == 0xe0));
}
#[test]
fn strict_polyphony_rejects_unrepresentable_expression_before_mutation() {
    let mut output = MidiOutput::new(
        DestinationId(1),
        OutputProfile::Polyphonic {
            channel: 0,
            voices: 1,
            expression: PolyExpression::RejectUnsupported,
        },
    )
    .unwrap();
    assert!(
        output
            .accept(event(
                1,
                on(
                    60,
                    Expression {
                        pitch: 1,
                        ..Expression::default()
                    }
                )
            ))
            .is_err()
    );
    output
        .accept(event(1, on(60, Expression::default())))
        .unwrap();
    assert!(
        output
            .accept(event(
                1,
                Gesture::Expression {
                    expression: Expression {
                        timbre: 65_535,
                        ..Expression::default()
                    }
                }
            ))
            .is_err()
    );
    assert!(
        output
            .accept(event(2, on(62, Expression::default())))
            .is_err()
    );
    output
        .accept(event(1, Gesture::Off { velocity: 0 }))
        .unwrap();
    output
        .accept(event(2, on(62, Expression::default())))
        .unwrap();
}
#[test]
fn monophonic_last_note_priority_restores_expression_and_never_releases_wrong_note() {
    let mut output = MidiOutput::new(
        DestinationId(1),
        OutputProfile::Monophonic {
            channel: 2,
            bend_semitones: 48,
        },
    )
    .unwrap();
    output
        .accept(event(1, on(60, Expression::default())))
        .unwrap();
    let switch = bytes(
        &output
            .accept(event(2, on(64, Expression::default())))
            .unwrap(),
    );
    assert_eq!(switch[0], [0x82, 60, 0]);
    assert_eq!(*switch.last().unwrap(), [0x92, 64, 90]);
    let expression = Expression {
        pitch: 24_000_000,
        timbre: 65_535,
        pressure: 65_535,
    };
    assert!(
        output
            .accept(event(1, Gesture::Expression { expression }))
            .unwrap()
            .messages()
            .is_empty()
    );
    let restore = bytes(
        &output
            .accept(event(2, Gesture::Off { velocity: 11 }))
            .unwrap(),
    );
    assert_eq!(restore[0], [0x82, 64, 11]);
    assert_eq!(*restore.last().unwrap(), [0x92, 60, 90]);
    assert!(restore.contains(&[0xe2, 0, 96]));
    assert!(restore.contains(&[0xb2, 74, 127]));
    assert!(restore.contains(&[0xd2, 127, 0]));
    output
        .accept(event(3, on(67, Expression::default())))
        .unwrap();
    assert!(
        output
            .accept(event(1, Gesture::Off { velocity: 0 }))
            .unwrap()
            .messages()
            .is_empty()
    );
    let cleanup = output.release_all();
    assert_eq!(cleanup[0].bytes(), [0x82, 67, 0]);
    assert!(!cleanup.iter().any(|m| m.bytes()[0] & 0xf0 == 0x90));
    output
        .accept(event(4, on(69, Expression::default())))
        .unwrap();
}
#[test]
fn monophonic_invalid_pitch_and_full_held_stack_preserve_the_audible_voice() {
    let mut output = MidiOutput::new(
        DestinationId(1),
        OutputProfile::Monophonic {
            channel: 0,
            bend_semitones: 2,
        },
    )
    .unwrap();
    for id in 1..=128 {
        output
            .accept(event(id, on(60, Expression::default())))
            .unwrap();
    }
    assert!(
        output
            .accept(event(129, on(65, Expression::default())))
            .is_err()
    );
    assert!(
        output
            .accept(event(
                128,
                Gesture::Expression {
                    expression: Expression {
                        pitch: 3_000_000,
                        ..Expression::default()
                    }
                }
            ))
            .is_err()
    );
    let off = bytes(
        &output
            .accept(event(128, Gesture::Off { velocity: 0 }))
            .unwrap(),
    );
    assert_eq!(off[0], [0x80, 60, 0]);
    assert_eq!(*off.last().unwrap(), [0x90, 60, 90]);
    assert!(off.contains(&[0xe0, 0, 64]));
}
