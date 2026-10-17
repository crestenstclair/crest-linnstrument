use super::*;
use crest_performance_core::{ExpressionPolicy, Gesture, HarmonyPlaying, SlotState};

fn runtime(model: u16) -> Runtime {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let flash = root.join(format!("target/firmware-native/ideal-{model}.bin"));
    let (worker, firmware) = Worker::start(
        &root.join("target/firmware-native/linnstrument-worker"),
        &flash,
        model,
    )
    .unwrap();
    let geometry = if model == 200 {
        Geometry::Pads200
    } else {
        Geometry::Pads128
    };
    let mut runtime = Runtime {
        run: GestureRun::new(&firmware, &flash).unwrap(),
        worker,
        firmware,
        host: TestHost::new(Session::default(), geometry).unwrap(),
        geometry,
        contacts: vec![],
        contact_profile: ContactProfile::default(),
        ramps: Vec::with_capacity(16),
        scenario: None,
        recent_midi: std::collections::VecDeque::new(),
        verified_steps: None,
        paused: true,
        connected: true,
        #[cfg(feature = "midi")]
        midi: None,
    };
    runtime.advance(1_000_000).unwrap();
    runtime
}

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn quantized_host_and_firmware_pad_launches_show_queued_then_playing_leds() {
    for model in [128, 200] {
        verify_quantized_launch(model);
    }
}

fn verify_quantized_launch(model: u16) {
    use crest_performance_core::{PatternSettings, TICKS_PER_BEAT};
    let mut r = runtime(model);
    r.command(BridgeCommand::Generate {
        slot: 0,
        settings: PatternSettings::default(),
    })
    .unwrap();
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    let boundary = (r.host.engine.position().0 / TICKS_PER_BEAT + 4) * TICKS_PER_BEAT;
    r.command(BridgeCommand::LaunchQuantum { ticks: boundary })
        .unwrap();
    r.command(BridgeCommand::Launch { slot: 0 }).unwrap();
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Queued);
    r.command(BridgeCommand::StopSlot { slot: 0 }).unwrap();
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Recorded);
    let captured = r.host.output_count();
    r.set_contact(Contact {
        id: 1,
        x: 1.0,
        y: 0.0,
        z: 0.5,
        latched: false,
    })
    .unwrap();
    r.advance(100_000).unwrap();
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(50_000).unwrap();
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Queued);
    assert_eq!(
        r.host.output_count(),
        captured,
        "queued clips must remain silent"
    );
    let [red, green, blue] = r.firmware.leds[8];
    assert!(
        red > 200 && green > 200 && blue < 50,
        "queued firmware LED must be yellow"
    );
    r.command(BridgeCommand::LaunchQuantum { ticks: 0 })
        .unwrap();
    // A preference change applies only to future launches, not this queued clip.
    while r.host.engine.position().0 < boundary {
        r.advance(25_000).unwrap();
    }
    r.advance(50_000).unwrap();
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Playing);
    let first_attack = r
        .host
        .output_since(captured)
        .unwrap()
        .iter()
        .find(|m| m.message.bytes()[0] & 0xf0 == 0x90)
        .unwrap();
    assert_eq!(first_attack.at, BeatTime(boundary));
    let [red, green, blue] = r.firmware.leds[8];
    assert!(
        green > 200 && red < 50 && blue < 50,
        "playing firmware LED must be green"
    );
    r.set_contact(Contact {
        id: 2,
        x: 1.0,
        y: 0.0,
        z: 0.5,
        latched: false,
    })
    .unwrap();
    r.advance(100_000).unwrap();
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(50_000).unwrap();
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Recorded);
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
}

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn queue_overflow_cannot_lose_a_release_or_replay_stale_gestures_after_reset() {
    let mut r = runtime(200);
    r.set_contact(Contact {
        id: 1,
        x: 4.0,
        y: 2.0,
        z: 0.5,
        latched: false,
    })
    .unwrap();
    r.advance(200_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 1);
    let (sender, receiver) = mpsc::sync_channel(1);
    let snapshot = Arc::new(Mutex::new(String::new()));
    let overflow = Arc::new(AtomicBool::new(false));
    let bridge = Bridge {
        sender,
        snapshot: snapshot.clone(),
        overflow: overflow.clone(),
    };
    bridge
        .send(BridgeCommand::Contact(Contact {
            id: 2,
            x: 9.0,
            y: 3.0,
            z: 0.5,
            latched: false,
        }))
        .unwrap();
    assert!(bridge.send(BridgeCommand::Release { id: 1 }).is_err());
    let scheduler = std::thread::spawn(move || {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        run_scheduler(
            r,
            receiver,
            snapshot,
            overflow,
            &root.join("target/firmware-native/linnstrument-worker"),
            &root.join("target/firmware-native/ideal-200.bin"),
            200,
        );
    });
    let failed = wait_bridge(&bridge, |s| s["connected"] == false);
    assert!(failed["error"].as_str().unwrap().contains("queue overflow"));
    assert_eq!(failed["contacts"], serde_json::json!([]));
    assert_eq!(failed["host"]["performance"]["active_voices"], 0);
    assert!(
        failed["host"]["recent_midi"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| {
                let message = &event["message"]["bytes"];
                message[0].as_u64().unwrap() & 0xf0 == 0x80 && message[1] == 43
            })
    );
    bridge.send(BridgeCommand::Reset {}).unwrap();
    let restored = wait_bridge(&bridge, |s| s["connected"] == true);
    assert_eq!(restored["contacts"], serde_json::json!([]));
    assert_eq!(restored["host"]["performance"]["active_voices"], 0);
    drop(bridge);
    scheduler.join().unwrap();
}
fn wait_bridge(
    bridge: &Bridge,
    predicate: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&bridge.snapshot().unwrap())
            && predicate(&value)
        {
            return value;
        }
        assert!(Instant::now() < deadline, "bridge snapshot timeout");
        std::thread::sleep(Duration::from_millis(5));
    }
}
#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn native_pressure_profiles_reach_sensors_and_safety_release_bypasses_tails() {
    let mut r = runtime(200);
    r.command(BridgeCommand::ContactProfile {
        profile: ContactProfile {
            attack_us: 100_000,
            release_us: 80_000,
        },
    })
    .unwrap();
    r.command(BridgeCommand::Contact(Contact {
        id: 1,
        x: 4.0,
        y: 2.0,
        z: 0.6,
        latched: false,
    }))
    .unwrap();
    assert_eq!(r.contacts[0].z, 0.0);
    r.advance(50_000).unwrap();
    assert!((r.contacts[0].z - 0.3).abs() < 0.02);
    assert_eq!(
        r.host.engine.snapshot().active_voices,
        1,
        "normal firmware touch path must detect ramped sensor pressure"
    );
    assert!(
        r.command(BridgeCommand::ContactProfile {
            profile: ContactProfile::default()
        })
        .is_err()
    );
    r.advance(60_000).unwrap();
    assert_eq!(r.contacts[0].z, 0.6);
    r.command(BridgeCommand::Release { id: 1 }).unwrap();
    r.advance(40_000).unwrap();
    assert!((r.contacts[0].z - 0.3).abs() < 0.02);
    r.advance(60_000).unwrap();
    assert!(r.contacts.is_empty() && r.ramps.is_empty());
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    r.command(BridgeCommand::Contact(Contact {
        id: 2,
        x: 9.0,
        y: 3.0,
        z: 0.6,
        latched: false,
    }))
    .unwrap();
    r.advance(100_000).unwrap();
    r.command(BridgeCommand::ReleaseMomentary {}).unwrap();
    assert!(r.contacts.is_empty() && r.ramps.is_empty());
    r.advance(30_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    r.set_contact(Contact {
        id: 3,
        x: 14.0,
        y: 4.0,
        z: 0.5,
        latched: false,
    })
    .unwrap();
    r.advance(100_000).unwrap();
    r.command(BridgeCommand::CancelContact { id: 3 }).unwrap();
    assert!(r.contacts.is_empty() && r.ramps.is_empty());
    r.advance(30_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert_eq!(
        r.run
            .verify(
                &root.join("target/firmware-native/linnstrument-worker"),
                &root.join("target/firmware-native/ideal-200.bin")
            )
            .unwrap(),
        r.run.steps()
    );
}
#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn scripted_contacts_survive_window_focus_changes_and_reject_manual_collisions() {
    let mut r = runtime(200);
    r.command(BridgeCommand::PlayExample {}).unwrap();
    r.advance(250_000).unwrap();
    r.command(BridgeCommand::ReleaseMomentary {}).unwrap();
    assert_eq!(r.contacts.len(), 3);
    assert!(
        r.command(BridgeCommand::Contact(Contact {
            id: 1,
            x: 1.0,
            y: 1.0,
            z: 0.5,
            latched: false
        }))
        .is_err()
    );
    r.advance(800_000).unwrap();
    assert!(r.contacts.is_empty());
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
}
#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn script_owns_release_tails_and_new_fingers_retire_tails_at_capacity() {
    let mut r = runtime(200);
    r.set_contact_profile(ContactProfile {
        attack_us: 0,
        release_us: 200_000,
    })
    .unwrap();
    let contact = Contact {
        id: 1,
        x: 4.0,
        y: 2.0,
        z: 0.5,
        latched: false,
    };
    r.start_scenario(ContactScenario {
        version: 1,
        model: 200,
        steps: vec![
            ContactStep {
                at_us: 0,
                action: ContactAction::Contact(contact),
            },
            ContactStep {
                at_us: 100_000,
                action: ContactAction::Release { id: 1 },
            },
        ],
    })
    .unwrap();
    r.advance(150_000).unwrap();
    assert!(r.scenario.as_ref().unwrap().steps.is_empty());
    assert_eq!(r.contacts.len(), 1);
    r.command(BridgeCommand::ReleaseMomentary {}).unwrap();
    assert!(r.command(BridgeCommand::Release { id: 1 }).is_err());
    assert_eq!(r.contacts.len(), 1);
    r.advance(200_000).unwrap();
    assert!(r.scenario.is_none() && r.contacts.is_empty());
    assert_eq!(r.host.engine.snapshot().active_voices, 0);

    // No time advances here: capacity and tail ownership are native contact
    // invariants, independent of firmware's separate MPE channel limit.
    for id in 1..=16 {
        r.set_contact(Contact {
            id,
            x: f64::from(id),
            ..contact
        })
        .unwrap();
    }
    let seventeenth = Contact {
        id: 17,
        x: 17.0,
        ..contact
    };
    assert!(r.set_contact(seventeenth).is_err());
    assert_eq!(r.contacts.len(), 16);
    r.release(1).unwrap();
    r.set_contact(seventeenth).unwrap();
    assert_eq!(r.contacts.len(), 16);
    assert!(!r.contacts.iter().any(|c| c.id == 1));
    assert!(r.contacts.iter().any(|c| c.id == 17));
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    assert!(r.contacts.is_empty() && r.ramps.is_empty());
}
#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_chord_phrase_preserves_three_gestures_and_launches_rendered_clip() {
    for model in [128, 200] {
        verify_chord_phrase(model);
    }
}
fn verify_chord_phrase(model: u16) {
    let mut r = runtime(model);
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    r.command(BridgeCommand::PlayExample {}).unwrap();
    r.advance(250_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 9);
    assert!(r.firmware.sensor_reads > 1000);
    r.advance(750_000).unwrap();
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    let session = r.host.engine.save_session().unwrap();
    let clip = session.slots[0].clip.as_ref().unwrap();
    let notes = clip.notes().unwrap();
    assert_eq!(notes.len(), 9);
    let mut played: Vec<_> = notes.iter().map(|n| n.key).collect();
    played.sort_unstable();
    assert_eq!(played, [64, 67, 69, 71, 71, 72, 74, 76, 77]);
    let mut inputs: Vec<_> = clip
        .harmony_origin
        .iter()
        .filter_map(|g| {
            if let Gesture::On { key, .. } = g.input.gesture {
                Some(key)
            } else {
                None
            }
        })
        .collect();
    inputs.sort_unstable();
    assert_eq!(
        inputs,
        [43, 53, 63],
        "the firmware played three notes; library rendered nine voices"
    );
    for chord in notes.chunks_exact(3) {
        assert_eq!(chord[0].expression, chord[1].expression);
        assert_eq!(chord[0].expression, chord[2].expression);
    }
    let bent = notes.chunks_exact(3).find(|c| c[0].key == 71).unwrap();
    let vertical = notes.chunks_exact(3).find(|c| c[0].key == 64).unwrap();
    let pressure = notes.chunks_exact(3).find(|c| c[0].key == 69).unwrap();
    assert!(
        bent[0]
            .expression
            .points
            .iter()
            .any(|p| p.value.pitch > 200_000)
    );
    assert!(
        vertical[0]
            .expression
            .points
            .iter()
            .all(|p| p.value.pitch.abs() < 100_000)
    );
    assert!(
        pressure[0]
            .expression
            .points
            .iter()
            .all(|p| p.value.pitch.abs() < 100_000)
    );
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let path = root.join(format!("target/firmware-native/harmony-{model}.json"));
    crest_performance_host::storage::save_session(&path, &session).unwrap();
    r.command(BridgeCommand::Open {
        path: path.to_string_lossy().into_owned(),
    })
    .unwrap();
    assert_eq!(r.host.engine.session(), &session);
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    r.set_contact(Contact {
        id: 99,
        x: 1.0,
        y: 0.0,
        z: 0.5,
        latched: false,
    })
    .unwrap();
    r.advance(200_000).unwrap();
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Playing);
    assert!(r.host.engine.snapshot().active_voices >= 3);
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    let led = r.firmware.leds[8];
    assert!(
        led[1] > led[0] && led[1] > led[2],
        "playing LED must come from firmware: {led:?}"
    );
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    r.host.disconnect().unwrap();
}
#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_strumplate_and_top_voice_policy_use_stock_mpe() {
    let mut r = runtime(200);
    r.command(BridgeCommand::Harmony {
        settings: HarmonySettings {
            playing: HarmonyPlaying::Strumplate,
            expression: ExpressionPolicy::TopVoice,
            ..HarmonySettings::default()
        },
    })
    .unwrap();
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    r.set_contact(Contact {
        id: 1,
        x: 1.0,
        y: 0.0,
        z: 0.5,
        latched: false,
    })
    .unwrap();
    r.advance(200_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 1);
    assert!(
        r.host
            .capture
            .iter()
            .any(|m| m.message.bytes()[0] & 0xf0 == 0x90 && m.message.bytes()[1] == 60)
    );
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
}

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_three_finger_phrase_records_timed_strums_for_both_geometries() {
    for model in [128, 200] {
        verify_timed_strum(model);
    }
}

fn verify_timed_strum(model: u16) {
    use crest_performance_core::{API_VERSION, Articulation, Engine, Limits, StrumDirection};
    let mut r = runtime(model);
    r.command(BridgeCommand::Harmony {
        settings: HarmonySettings {
            articulation: Articulation::Strum {
                direction: StrumDirection::Ascending,
                interval: BeatTime(120_000),
            },
            ..HarmonySettings::default()
        },
    })
    .unwrap();
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    r.command(BridgeCommand::PlayExample {}).unwrap();
    r.advance(1_000_000).unwrap();
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    assert!(r.firmware.sensor_reads > 1000);
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    let session = r.host.engine.save_session().unwrap();
    let clip = session.slots[0].clip.as_ref().unwrap();
    let notes = clip.notes().unwrap();
    assert_eq!(notes.len(), 9);
    let triggers: Vec<_> = clip
        .harmony_origin
        .iter()
        .filter_map(|origin| match origin.input.gesture {
            Gesture::On { key, .. } => Some((origin.input.at, key)),
            _ => None,
        })
        .collect();
    assert_eq!(
        triggers.len(),
        3,
        "Three actual firmware attacks must produce nine library voices"
    );
    for (at, key) in triggers {
        let pitches = match key {
            43 => [71, 74, 77],
            53 => [64, 67, 71],
            63 => [69, 72, 76],
            _ => panic!("Unexpected firmware pitch {key}"),
        };
        for (step, key) in pitches.into_iter().enumerate() {
            let start = BeatTime(at.0 + step as u64 * 120_000);
            assert_eq!(
                notes
                    .iter()
                    .filter(|note| note.key == key && note.start == start)
                    .count(),
                1
            );
        }
    }
    assert!(notes.iter().any(|note| {
        note.expression
            .points
            .iter()
            .any(|point| point.value.pitch > 200_000)
    }));
    let reopened: Session = serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
    assert_eq!(reopened, session);
    let mut replay = Engine::new(API_VERSION, reopened, Limits::default()).unwrap();
    replay
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    replay.advance(BeatTime(clip.length.0 - 1)).unwrap();
    let played: Vec<_> = replay.drain_output().collect();
    assert_eq!(
        played
            .iter()
            .map(|event| (event.at, event.gesture))
            .collect::<Vec<_>>(),
        clip.events
            .iter()
            .map(|event| (event.at, event.gesture))
            .collect::<Vec<_>>()
    );
    r.host.disconnect().unwrap();
}

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_retrospective_capture_keeps_three_held_inputs_live() {
    let mut r = runtime(200);
    for (id, x, y) in [(1, 1.0, 0.0), (2, 4.0, 1.0), (3, 7.0, 2.0)] {
        r.set_contact(Contact {
            id,
            x,
            y,
            z: 0.5,
            latched: true,
        })
        .unwrap();
    }
    r.advance(500_000).unwrap();
    for contact in [
        Contact {
            id: 1,
            x: 1.3,
            y: 0.0,
            z: 0.5,
            latched: true,
        },
        Contact {
            id: 2,
            x: 4.0,
            y: 1.3,
            z: 0.5,
            latched: true,
        },
        Contact {
            id: 3,
            x: 7.0,
            y: 2.0,
            z: 0.8,
            latched: true,
        },
    ] {
        r.set_contact(contact).unwrap();
    }
    r.advance(600_000).unwrap();
    let at = r.host.engine.position();
    let captured_messages = r.host.output_count();
    r.command(BridgeCommand::CaptureRecent {
        slot: 0,
        ticks: 4 * crest_performance_core::TICKS_PER_BEAT,
    })
    .unwrap();
    assert_eq!(r.host.engine.position(), at);
    assert_eq!(r.host.output_count(), captured_messages);
    assert_eq!(r.host.engine.snapshot().active_voices, 3);
    let clip = r.host.engine.session().slots[0].clip.clone().unwrap();
    let notes = clip.notes().unwrap();
    assert_eq!(notes.len(), 3);
    let bend = notes.iter().find(|note| note.key == 30).unwrap();
    let timbre = notes.iter().find(|note| note.key == 38).unwrap();
    let pressure = notes.iter().find(|note| note.key == 46).unwrap();
    assert!(bend.expression.points.last().unwrap().value.pitch > 200_000);
    assert!(timbre.expression.points.last().unwrap().value.timbre > 40_000);
    assert!(pressure.expression.points.last().unwrap().value.pressure > 40_000);
    r.advance(200_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 3);
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    assert_eq!(r.host.engine.session().slots[0].clip.as_ref(), Some(&clip));
}

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_played_voicing_learns_records_and_launches_on_both_geometries() {
    for model in [128, 200] {
        verify_learned_voicing(model);
    }
}

fn verify_learned_voicing(model: u16) {
    let mut r = runtime(model);
    for (id, x) in [(1, 2.0), (2, 6.0), (3, 9.0)] {
        r.set_contact(Contact {
            id,
            x,
            y: 2.0,
            z: 0.5,
            latched: true,
        })
        .unwrap();
    }
    r.advance(300_000).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 3);
    r.command(BridgeCommand::LearnChord {
        slot: 0,
        degree: 0,
        root: None,
    })
    .unwrap();
    let learned = r.host.engine.session().harmony.learned[0].unwrap();
    assert_eq!(learned.notes(), [41, 45, 48]);
    assert_eq!(learned.root(), 41);
    assert!(r.firmware.sensor_reads > 1000);
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    r.set_contact(Contact {
        id: 4,
        x: 1.0,
        y: 0.0,
        z: 0.5,
        latched: true,
    })
    .unwrap();
    r.advance(200_000).unwrap();
    r.set_contact(Contact {
        id: 4,
        x: 1.3,
        y: 0.3,
        z: 0.8,
        latched: true,
    })
    .unwrap();
    r.advance(300_000).unwrap();
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(100_000).unwrap();
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    let saved = r.host.engine.save_session().unwrap();
    let reopened: Session = serde_json::from_slice(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(reopened, saved);
    let notes = saved.slots[0].clip.as_ref().unwrap().notes().unwrap();
    assert_eq!(
        notes.iter().map(|n| n.key).collect::<Vec<_>>(),
        [41, 45, 48]
    );
    for note in &notes {
        assert!(
            note.expression
                .points
                .iter()
                .any(|p| p.value.pitch > 200_000
                    && p.value.timbre > 40_000
                    && p.value.pressure > 40_000)
        );
    }
    r.command(BridgeCommand::Mode { clips: true }).unwrap();
    r.set_contact(Contact {
        id: 5,
        x: 1.0,
        y: 0.0,
        z: 0.5,
        latched: false,
    })
    .unwrap();
    r.advance(150_000).unwrap();
    r.command(BridgeCommand::ReleaseAll {}).unwrap();
    r.advance(50_000).unwrap();
    assert_eq!(r.host.engine.snapshot().slots[0], SlotState::Playing);
    let [red, green, blue] = r.firmware.leds[8];
    assert!(green > 200 && red < 50 && blue < 50, "{red} {green} {blue}");
    r.command(BridgeCommand::Stop {}).unwrap();
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
}

#[test]
#[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
fn firmware_three_finger_arpeggios_record_exact_gates_and_replay_on_both_geometries() {
    for model in [128, 200] {
        verify_firmware_arpeggio(model);
    }
}

fn verify_firmware_arpeggio(model: u16) {
    use crest_performance_core::{API_VERSION, Articulation, Engine, Limits, StrumDirection};
    let mut r = runtime(model);
    r.command(BridgeCommand::Harmony {
        settings: HarmonySettings {
            articulation: Articulation::Arpeggio {
                direction: StrumDirection::Ascending,
                interval: BeatTime(120_000),
                gate: BeatTime(90_000),
            },
            ..HarmonySettings::default()
        },
    })
    .unwrap();
    r.command(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })
    .unwrap();
    r.command(BridgeCommand::BeginRecord { slot: 0 }).unwrap();
    r.command(BridgeCommand::PlayExample {}).unwrap();
    r.advance(1_000_000).unwrap();
    r.command(BridgeCommand::FinishRecord {}).unwrap();
    assert!(r.firmware.sensor_reads > 1000);
    assert_eq!(r.host.engine.snapshot().active_voices, 0);
    let session = r.host.engine.save_session().unwrap();
    let clip = session.slots[0].clip.as_ref().unwrap();
    assert_arpeggio_gates(clip);
    let notes = clip.notes().unwrap();
    assert!(
        notes.len() > 9,
        "firmware triggers must survive through repeated cycles"
    );
    assert!(notes.iter().any(|note| {
        note.expression
            .points
            .iter()
            .any(|point| point.value.pitch > 200_000)
    }));
    let reopened: Session = serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
    assert_eq!(reopened, session);
    let mut replay = Engine::new(API_VERSION, reopened, Limits::default()).unwrap();
    replay
        .command(Command::Launch {
            slot: 0,
            quantum: BeatTime(0),
        })
        .unwrap();
    replay.advance(BeatTime(clip.length.0 - 1)).unwrap();
    let played: Vec<_> = replay
        .drain_output()
        .map(|event| (event.at, event.gesture))
        .collect();
    assert_eq!(
        played,
        clip.events
            .iter()
            .map(|event| (event.at, event.gesture))
            .collect::<Vec<_>>()
    );
    r.host.disconnect().unwrap();
}

fn assert_arpeggio_gates(clip: &crest_performance_core::Clip) {
    let triggers: Vec<_> = clip
        .harmony_origin
        .iter()
        .filter(|origin| matches!(origin.input.gesture, Gesture::On { .. }))
        .collect();
    assert_eq!(triggers.len(), 3, "exactly three actual firmware triggers");
    let mut expected = Vec::new();
    for trigger in triggers {
        expected.extend(expected_arpeggio_notes(clip, trigger.input));
    }
    expected.sort();
    let mut actual: Vec<_> = clip
        .notes()
        .unwrap()
        .iter()
        .map(|note| (note.start.0, note.key, note.duration.0))
        .collect();
    actual.sort();
    assert_eq!(actual, expected);
}

fn expected_arpeggio_notes(
    clip: &crest_performance_core::Clip,
    input: crest_performance_core::InputEvent,
) -> Vec<(u64, u8, u64)> {
    let Gesture::On { key, .. } = input.gesture else {
        unreachable!()
    };
    let pitches = match key {
        43 => [71, 74, 77],
        53 => [64, 67, 71],
        63 => [69, 72, 76],
        _ => panic!("unexpected firmware pitch {key}"),
    };
    let release = clip
        .harmony_origin
        .iter()
        .find(|origin| {
            origin.input.note == input.note && matches!(origin.input.gesture, Gesture::Off { .. })
        })
        .unwrap()
        .input
        .at
        .0;
    (input.at.0..=release)
        .step_by(120_000)
        .enumerate()
        .map(|(step, at)| (at, pitches[step % 3], 90_000.min(release - at)))
        .collect()
}
