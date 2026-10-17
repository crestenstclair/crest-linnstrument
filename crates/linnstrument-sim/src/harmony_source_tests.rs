use super::*;
use crest_performance_core::{DestinationId, Gesture, InstrumentTarget, SlotState, Track, TrackId};

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_chord_clip_drives_independent_follower_and_stopping_restores_selected_harmony() {
    for model in [128, 200] {
        verify_source(model);
    }
}
fn touch(r: &mut Runtime, column: f64) {
    touch_at(r, column, 0.0);
}
fn touch_at(r: &mut Runtime, column: f64, row: f64) {
    r.set_contact(Contact {
        id: 1,
        x: column,
        y: row,
        z: 0.55,
        latched: false,
    })
    .unwrap();
    r.advance(150_000).unwrap();
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
}
fn verify_source(model: u16) {
    let mut session = Session::default();
    session.tracks.push(Track {
        id: TrackId(2),
        name: "Follower".into(),
        instrument: InstrumentTarget {
            destination: DestinationId(2),
            reference: "capture://follower".into(),
        },
    });
    session.slots[1].track = TrackId(2);
    let mut r = super::harmony_tests::runtime_with_session(model, session);
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    touch(&mut r, 1.0);
    touch(&mut r, 5.0);
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    let leader = r.host.engine.session().slots[0].clip.clone().unwrap();
    let changes: Vec<_> = leader
        .harmony_origin
        .iter()
        .filter(|origin| matches!(origin.input.gesture, Gesture::On { .. }))
        .collect();
    assert_eq!(
        changes
            .iter()
            .map(|origin| origin.settings.degree)
            .collect::<Vec<_>>(),
        [0, 4]
    );
    assert_eq!(leader.notes().unwrap().len(), 6);
    assert!(r.firmware.sensor_reads > 1000);
    configure_and_reopen(&mut r, model);
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    touch(&mut r, 1.0);
    let start = r.host.engine.slot_launch_time(0).unwrap().unwrap();
    let before = r.host.output_count();
    touch(&mut r, 2.0);
    r.advance(600_000).unwrap();
    r.advance(500_000).unwrap();
    verify_follower(
        &r,
        before,
        start,
        &leader,
        changes[0].input.at,
        changes[1].input.at,
    );
    assert_eq!(
        &r.host.engine.snapshot().slots[..2],
        &[SlotState::Playing; 2]
    );
    assert!(r.firmware.leds[8][1] > 200 && r.firmware.leds[16][1] > 200);
    verify_muted_leader(&mut r);
    verify_controller_harmony(&mut r);
    touch(&mut r, 1.0);
    assert_eq!(
        &r.host.engine.snapshot().slots[..2],
        &[SlotState::Recorded, SlotState::Playing]
    );
    let before = r.host.output_count();
    r.advance(200_000).unwrap();
    let notes = follower_notes(&r, before);
    assert!(!notes.is_empty());
    assert!(notes.iter().all(|(_, key)| *key == 62));
    assert!(r.firmware.leds[8][2] > 200 && r.firmware.leds[16][1] > 200);
    r.command(BridgeCommand::Stop {}).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
}
fn verify_controller_harmony(r: &mut Runtime) {
    let original = r.host.engine.session().slots.clone();
    let sensor_reads = r.firmware.sensor_reads;
    for (degree, expected) in [60, 62, 64, 65, 67, 69, 71].into_iter().enumerate() {
        touch_at(r, degree as f64 + 1.0, 7.0);
        let state = r.host.engine.snapshot();
        assert_eq!(state.harmony_source, HarmonySource::Selected {});
        assert_eq!(state.effective_harmony.degree, degree as u8);
        assert_eq!(&state.slots[..2], &[SlotState::Playing; 2]);
        let before = r.host.output_count();
        r.advance(200_000).unwrap();
        let notes = follower_notes(r, before);
        assert!(!notes.is_empty());
        assert!(notes.iter().all(|(_, key)| *key == expected));
        verify_degree_leds(r, degree);
    }
    assert!(r.firmware.sensor_reads > sensor_reads + 1000);
    assert_eq!(r.host.engine.session().slots, original);
    // Restore the recorded source to exercise its independent stop/fallback.
    r.command(BridgeCommand::Harmony {
        settings: HarmonySettings {
            degree: 1,
            ..HarmonySettings::default()
        },
    })
    .unwrap();
    r.command(BridgeCommand::HarmonySource {
        source: HarmonySource::Clip { slot: 0 },
    })
    .unwrap();
}
fn verify_degree_leds(r: &Runtime, active: usize) {
    for degree in 0..7 {
        let [red, green, blue] = r.firmware.leds[(degree + 1) * 8 + 7];
        assert!(red < 50);
        if degree == active {
            assert!(green > 200 && blue < 50, "selected degree is green");
        } else {
            assert!(green < 50 && blue > 200, "other degrees are blue");
        }
    }
}
fn verify_muted_leader(r: &mut Runtime) {
    let harmony = r.host.engine.snapshot().effective_harmony;
    r.command(BridgeCommand::ClipMuted {
        slot: 0,
        muted: true,
    })
    .unwrap();
    assert_eq!(r.host.engine.snapshot().effective_harmony, harmony);
    let before = r.host.output_count();
    r.advance(600_000).unwrap();
    assert!(
        r.host
            .output_since(before)
            .unwrap()
            .iter()
            .all(|m| m.destination != DestinationId(1))
    );
    let notes = follower_notes(r, before);
    assert!(notes.iter().any(|(_, key)| *key == 60));
    assert!(notes.iter().any(|(_, key)| *key == 67));
    let [red, green, blue] = r.firmware.leds[8];
    assert!(
        red < 50 && green > 200 && blue > 200,
        "muted firmware LED is cyan"
    );
    r.command(BridgeCommand::ClipMuted {
        slot: 0,
        muted: false,
    })
    .unwrap();
    let before = r.host.output_count();
    r.advance(600_000).unwrap();
    assert!(
        r.host
            .output_since(before)
            .unwrap()
            .iter()
            .any(|m| m.destination == DestinationId(1) && m.message.bytes()[0] & 0xf0 == 0x90)
    );
    assert!(!r.host.engine.snapshot().muted_slots[0]);
    let [red, green, blue] = r.firmware.leds[8];
    assert!(
        red < 50 && green > 200 && blue < 50,
        "released mute restores green"
    );
}
fn configure_and_reopen(r: &mut Runtime, model: u16) {
    r.command(BridgeCommand::Harmony {
        settings: HarmonySettings::default(),
    })
    .unwrap();
    r.command(BridgeCommand::Generate {
        slot: 1,
        settings: PatternSettings {
            steps: 8,
            pulses: 8,
            step: BeatTime(48000),
            gate: BeatTime(24000),
            tone_min: 0,
            tone_max: 0,
            follow_harmony: true,
            ..PatternSettings::default()
        },
    })
    .unwrap();
    r.command(BridgeCommand::Harmony {
        settings: HarmonySettings {
            degree: 1,
            ..HarmonySettings::default()
        },
    })
    .unwrap();
    r.command(BridgeCommand::HarmonySource {
        source: HarmonySource::Clip { slot: 0 },
    })
    .unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../target/firmware-native/harmony-source-{model}.json"
    ));
    r.command(BridgeCommand::Save {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    let saved = r.host.engine.save_session().unwrap();
    r.command(BridgeCommand::Open {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(r.host.engine.session(), &saved);
}
fn follower_notes(r: &Runtime, after: u64) -> Vec<(BeatTime, u8)> {
    r.host
        .output_since(after)
        .unwrap()
        .iter()
        .filter(|m| m.destination == DestinationId(2) && m.message.bytes()[0] & 0xf0 == 0x90)
        .map(|m| (m.at, m.message.bytes()[1]))
        .collect()
}
fn verify_follower(
    r: &Runtime,
    after: u64,
    start: BeatTime,
    leader: &crest_performance_core::Clip,
    first: BeatTime,
    second: BeatTime,
) {
    let notes = follower_notes(r, after);
    assert!(notes.len() > 20);
    assert!(notes.iter().any(|(_, key)| *key == 60));
    assert!(notes.iter().any(|(_, key)| *key == 67));
    for (at, key) in notes {
        let elapsed = at.0 - start.0;
        let phase = elapsed % leader.length.0;
        let expected = if phase >= second.0 || (phase < first.0 && elapsed >= leader.length.0) {
            67
        } else if phase >= first.0 {
            60
        } else {
            62
        };
        assert_eq!(key, expected, "follower at {at:?}");
    }
}
