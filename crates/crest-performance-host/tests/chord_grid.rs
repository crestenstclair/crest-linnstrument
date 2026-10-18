//! Public host contract: firmware coordinate packets, not a firmware substitute.
use crest_linnstrument::Geometry;
use crest_performance_core::*;
use crest_performance_host::TestHost;
fn host(geometry: Geometry) -> TestHost {
    let mut session = Session::default();
    session.harmony.input_octaves = true;
    session.harmony.input_anchor = 28;
    let mut host = TestHost::new(session, geometry).unwrap();
    host.command(Command::SetMode(SurfaceMode::Chords)).unwrap();
    host
}
fn midi(host: &mut TestHost, bytes: [u8; 3]) {
    for byte in bytes {
        host.accept_bytes(0, byte, Geometry::Pads200).unwrap();
    }
}
fn notes(host: &TestHost) -> Vec<u8> {
    host.capture
        .iter()
        .filter_map(|m| {
            let [s, k, v] = m.message.bytes();
            (s & 0xf0 == 0x90 && v > 0).then_some(k)
        })
        .collect()
}
#[test]
fn one_degree_bank_preserves_octaves_and_independent_ownership() {
    for (geometry, first) in [(Geometry::Pads128, 10), (Geometry::Pads200, 14)] {
        let mut h = host(geometry);
        let help = h.snapshot().pad_help;
        let playing: Vec<_> = help
            .iter()
            .filter(|p| p.title.starts_with("Chord "))
            .collect();
        assert_eq!(
            playing.len(),
            49,
            "exactly seven degrees across seven octave rows"
        );
        assert!(
            playing
                .iter()
                .all(|p| p.column >= first && p.column < first + 7)
        );
        assert!(
            !help.iter().any(|p| p.column == first - 1),
            "dark separator"
        );
        assert!(help.iter().all(|p| p.column < geometry.columns()));
        for column in 1..first {
            midi(&mut h, [0x93, column, 90]); // reclaimed area has no hidden playing bank
            midi(&mut h, [0x83, column, 0]);
        }
        assert!(notes(&h).is_empty());
        midi(&mut h, [0x91, first, 90]); // C0
        midi(&mut h, [0x95, first, 90]); // C4
        midi(&mut h, [0x95, first + 6, 90]); // B4 diminished, rightmost pad
        assert_eq!(notes(&h), [12, 16, 19, 60, 64, 67, 71, 74, 77]);
        midi(&mut h, [0x85, first, 0]);
        assert_eq!(h.engine.snapshot().active_voices, 6);
        midi(&mut h, [0x81, first, 0]);
        midi(&mut h, [0x85, first + 6, 0]);
        assert_eq!(h.engine.snapshot().active_voices, 0);
    }
}
#[test]
fn unused_cells_are_inert_and_controls_apply_only_to_future_chords() {
    let mut h = host(Geometry::Pads128);
    midi(&mut h, [0x95, 10, 90]);
    for packet in [
        [0x93, 4, 90],
        [0x93, 8, 90],
        [0x95, 9, 90],
        [0x96, 3, 90],
        [0x96, 3, 90],
        [0x86, 3, 0],
        [0x97, 8, 90],
        [0x87, 8, 0],
    ] {
        midi(&mut h, packet);
    }
    assert_eq!(h.engine.snapshot().harmony.inversion, 1);
    assert_eq!(
        h.engine.snapshot().harmony.extension,
        ChordExtension::AddNinth
    );
    assert_eq!(notes(&h), [60, 64, 67]);
    midi(&mut h, [0x95, 11, 90]);
    assert_eq!(&notes(&h)[3..], &[65, 69, 74, 76]); // Dm add9 first inversion
    assert!(
        !h.snapshot()
            .pad_help
            .iter()
            .any(|p| p.column == 9 && p.row == 5)
    );
    h.command(Command::Stop).unwrap();
    assert_eq!(h.engine.snapshot().active_voices, 0);
}
#[test]
fn independent_cell_xyz_survives_channel_reallocation() {
    let mut h = host(Geometry::Pads200);
    for col in [14, 16, 18] {
        midi(&mut h, [0x95, col, 90]);
    }
    let before = h.capture.len();
    midi(&mut h, [0xb5, 46, 0]);
    midi(&mut h, [0xb5, 14, 8]); // A origin X=1024
    midi(&mut h, [0xb5, 46, 43]);
    midi(&mut h, [0xb5, 14, 9]); // +171, ~1 semitone
    midi(&mut h, [0xb5, 80, 100]); // B timbre
    midi(&mut h, [0xa5, 18, 90]); // C pressure
    let emitted: Vec<_> = h.capture[before..]
        .iter()
        .map(|m| m.message.bytes())
        .collect();
    let bends: Vec<_> = emitted
        .iter()
        .filter(|b| b[0] & 0xf0 == 0xe0 && (b[1] != 0 || b[2] != 64))
        .map(|b| b[0] & 15)
        .collect();
    assert_eq!(bends, [1, 2, 3]);
    let timbres: Vec<_> = emitted
        .iter()
        .filter(|b| b[0] & 0xf0 == 0xb0 && b[1] == 74 && b[2] == 100)
        .map(|b| b[0] & 15)
        .collect();
    assert_eq!(timbres, [4, 5, 6]);
    let pressures: Vec<_> = emitted
        .iter()
        .filter(|b| b[0] & 0xf0 == 0xd0 && b[1] == 90)
        .map(|b| b[0] & 15)
        .collect();
    assert_eq!(pressures, [7, 8, 9]);
    for col in [14, 16, 18] {
        midi(&mut h, [0x85, col, 0]);
    }
    assert_eq!(h.engine.snapshot().active_voices, 0);
}
#[test]
fn chromatic_palette_and_grid_settings_round_trip_without_changing_old_sessions() {
    let mut h = host(Geometry::Pads200);
    midi(&mut h, [0x97, 6, 90]); // explicit minor
    midi(&mut h, [0x97, 11, 90]); // major seventh
    midi(&mut h, [0x95, 14, 90]);
    assert_eq!(notes(&h), [60, 63, 67, 71]);
    h.command(Command::Stop).unwrap();
    let saved = h.document().unwrap();
    let json = serde_json::to_string(&saved).unwrap();
    let reopened =
        TestHost::from_document(serde_json::from_str(&json).unwrap(), Geometry::Pads200).unwrap();
    assert_eq!(reopened.engine.session(), &saved.session);
    let old = Session {
        version: 10,
        ..Session::default()
    };
    assert_eq!(old.migrate().unwrap().harmony, HarmonySettings::default());
    let mut invalid = saved.session;
    invalid.version = 10;
    assert!(invalid.migrate().is_err());
}
#[test]
fn extension_formulas_are_independent_expectations() {
    let s = HarmonySettings::default();
    for (extension, expected) in [
        (ChordExtension::AddNinth, vec![60, 64, 67, 74]),
        (ChordExtension::Ninth, vec![60, 64, 67, 71, 74]),
        (ChordExtension::Sixth, vec![60, 64, 67, 69]),
        (ChordExtension::MinorSeventh, vec![60, 64, 67, 70]),
        (ChordExtension::MajorNinth, vec![60, 64, 67, 71, 74]),
    ] {
        assert_eq!(
            HarmonySettings { extension, ..s }
                .voicing(0, None)
                .unwrap()
                .notes(),
            expected
        );
    }
}

#[test]
fn recorded_grid_phrase_keeps_expression_after_reopen_beside_live_chords() {
    let mut h = host(Geometry::Pads200);
    h.command(Command::BeginRecord { slot: 0 }).unwrap();
    midi(&mut h, [0x95, 14, 90]);
    h.advance_micros(100_000).unwrap();
    for packet in [[0xb5, 78, 100], [0xa5, 14, 90], [0x85, 14, 0]] {
        for byte in packet {
            h.accept_bytes(100_000, byte, Geometry::Pads200).unwrap();
        }
    }
    h.advance_micros(250_000).unwrap();
    h.command(Command::FinishRecord).unwrap();
    let document = h.document().unwrap();
    let original = document.session.slots[0].clip.as_ref().unwrap().clone();
    assert!(original.events.iter().any(
        |event| matches!(event.gesture,Gesture::Expression {expression} if expression.timbre>50_000)
    ));
    let mut reopened = TestHost::from_document(document, Geometry::Pads200).unwrap();
    reopened
        .command(Command::SetMode(SurfaceMode::Chords))
        .unwrap();
    midi(&mut reopened, [0x95, 16, 90]);
    reopened.launch(0).unwrap();
    reopened.advance_micros(0).unwrap();
    assert_eq!(reopened.engine.snapshot().active_voices, 6);
    reopened.advance_micros(100_000).unwrap();
    assert_eq!(reopened.engine.snapshot().active_voices, 3);
    assert_eq!(
        reopened.engine.session().slots[0].clip.as_ref().unwrap(),
        &original
    );
    reopened.disconnect().unwrap();
    assert_eq!(reopened.engine.snapshot().active_voices, 0);
    for byte in [0x95, 16, 90] {
        reopened
            .accept_bytes(100_000, byte, Geometry::Pads200)
            .unwrap();
    }
    assert_eq!(
        reopened.engine.snapshot().active_voices,
        3,
        "disconnect clears cell ownership"
    );
}

#[test]
fn legacy_labels_reject_new_harmony_in_generated_and_following_clips() {
    let mut h = host(Geometry::Pads200);
    h.command(Command::Generate {
        slot: 0,
        settings: PatternSettings::default(),
    })
    .unwrap();
    let mut session = h.document().unwrap().session;
    session.harmony = HarmonySettings::default();
    session.version = 10;
    assert!(session.clone().migrate().is_err());
    let clip = session.slots[0].clip.as_mut().unwrap();
    let mut origin = clip.pattern_origin[0];
    origin.algorithm_version = 4;
    assert!(origin.validate().is_err());
    clip.pattern_origin.clear();
    clip.follow = Some(HarmonyFollow {
        reference: HarmonySettings {
            quality: ChordQuality::Minor,
            ..HarmonySettings::default()
        },
    });
    assert!(session.migrate().is_err());
}
#[test]
fn chromatic_quality_uses_selected_scale_roots_even_after_borrowing() {
    let settings = HarmonySettings {
        quality: ChordQuality::Major,
        borrowed: true,
        ..HarmonySettings::default()
    };
    assert_eq!(settings.voicing(6, None).unwrap().notes(), [71, 75, 78]);
}

#[test]
fn flower_control_groups_are_adjacent_on_both_sizes() {
    for (geometry, first) in [(Geometry::Pads128, 1), (Geometry::Pads200, 5)] {
        let h = host(geometry);
        let help = h.snapshot().pad_help;
        for (row, labels) in [
            (7, ["Dim", "Min", "Maj", "Sus4", "6", "m7", "Maj7", "Add9"]),
            (
                6,
                [
                    "Inv−", "Inv0", "Inv+", "Lead", "Oct−", "Oct0", "Oct+", "Spread",
                ],
            ),
        ] {
            for (offset, label) in labels.into_iter().enumerate() {
                let pad = help
                    .iter()
                    .find(|p| p.column == first + offset as u8 && p.row == row)
                    .unwrap();
                assert_eq!(
                    pad.label, label,
                    "Flower group ordering and no gaps between controls"
                );
            }
        }
        assert!(
            help.iter()
                .all(|p| p.column >= first && p.column < first + 16),
            "same compact two-hand footprint on both models"
        );
    }
}
