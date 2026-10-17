use super::*;
use crest_performance_core::{Articulation, Gesture, SlotState, StrumDirection};

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_articulation_pads_select_record_reopen_and_launch_with_decoded_lights() {
    for model in [128, 200] {
        verify_articulation_pads(model);
    }
}

fn touch(r: &mut Runtime, column: u8, row: u8) {
    r.set_contact(Contact {
        id: 1,
        x: column.into(),
        y: row.into(),
        z: 0.55,
        latched: false,
    })
    .unwrap();
    r.advance(150_000).unwrap();
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(80_000).unwrap();
    // The host deliberately sends at most 16 changed cells per scheduler turn.
    // A full mode repaint must progress through turns, not one large time jump.
    for _ in 0..20 {
        r.advance(5_000).unwrap();
    }
    // Observe a complete multiplexing interval after the last LED write.
    r.advance(80_000).unwrap();
}

fn verify_selection(r: &mut Runtime, selected: u8) {
    touch(r, selected, 5);
    let articulation = r.host.engine.snapshot().harmony.articulation;
    match selected {
        6 => assert_eq!(articulation, Articulation::Block),
        7 | 8 => assert_eq!(
            articulation,
            Articulation::Strum {
                direction: if selected == 7 {
                    StrumDirection::Ascending
                } else {
                    StrumDirection::Descending
                },
                interval: BeatTime(60_000),
            }
        ),
        9 | 10 => assert_eq!(
            articulation,
            Articulation::Arpeggio {
                direction: if selected == 9 {
                    StrumDirection::Ascending
                } else {
                    StrumDirection::Descending
                },
                interval: BeatTime(60_000),
                gate: BeatTime(30_000),
            }
        ),
        11 => assert_eq!(
            articulation,
            Articulation::Humanized {
                max_delay: BeatTime(60_000),
                velocity_variation: 8,
                seed: 42,
            }
        ),
        _ => unreachable!(),
    }
    for column in 6..=11 {
        let [red, green, blue] = r.firmware.leds[column * 8 + 5];
        assert!(red < 50);
        if column == usize::from(selected) {
            assert!(green > 200 && blue < 50, "selected articulation {selected}");
        } else {
            assert!(
                blue > 200 && green < 50,
                "unselected articulation {column}, selected {selected}: {red}/{green}/{blue}"
            );
        }
    }
}

fn verify_articulation_pads(model: u16) {
    let mut r = super::harmony_tests::runtime(model);
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    let reads = r.firmware.sensor_reads;
    for selected in [7, 8, 9, 10, 11, 6, 8] {
        verify_selection(&mut r, selected);
    }
    assert!(r.firmware.sensor_reads > reads + 1000);
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    touch(&mut r, 1, 0);
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    let session = r.host.engine.save_session().unwrap();
    let clip = session.slots[0].clip.as_ref().unwrap();
    let attacks: Vec<_> = clip
        .events
        .iter()
        .filter_map(|event| {
            if let Gesture::On { key, .. } = event.gesture {
                Some((event.at.0, key))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        attacks.iter().map(|(_, key)| *key).collect::<Vec<_>>(),
        [67, 64, 60]
    );
    assert_eq!(attacks[1].0 - attacks[0].0, 60_000);
    assert_eq!(attacks[2].0 - attacks[1].0, 60_000);
    assert_eq!(clip.notes().unwrap().len(), 3);
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/firmware-native/articulation-pads-{model}.json"
    ));
    r.command(BridgeCommand::Save {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    r.command(BridgeCommand::Open {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(r.host.engine.session(), &session);
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    touch(&mut r, 1, 0);
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Playing);
    assert!(r.firmware.leds[8][1] > 200);
    assert!(r.firmware.leds[8 * 8 + 5][1] > 200);
    r.command(BridgeCommand::Stop {}).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
}
