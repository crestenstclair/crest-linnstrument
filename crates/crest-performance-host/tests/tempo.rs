use crest_linnstrument::Geometry;
use crest_performance_core::*;
use crest_performance_host::{
    TestHost,
    storage::{self, HOST_VERSION, HostDocument},
};

fn midi(host: &mut TestHost, micros: u64, bytes: &[u8]) {
    for byte in bytes {
        host.accept_bytes(micros, *byte, Geometry::Pads200).unwrap();
    }
}

#[test]
fn tempo_changes_preserve_live_expression_and_recording_in_musical_time() {
    let mut h = TestHost::new(Session::default(), Geometry::Pads200).unwrap();
    h.command(Command::BeginRecord { slot: 0 }).unwrap();
    midi(&mut h, 0, &[0x91, 60, 90]);
    midi(&mut h, 250_000, &[0xe1, 0, 65]);
    let count = h.output_count();
    h.set_tempo(500_000, 60_000).unwrap();
    assert_eq!(h.engine.position(), BeatTime(960_000));
    assert_eq!(
        h.output_count(),
        count,
        "tempo does not release or retune live notes"
    );
    assert_eq!(h.engine.snapshot().active_voices, 1);
    midi(&mut h, 1_000_000, &[0x81, 60, 17]);
    h.advance_micros(1_500_000).unwrap();
    h.command(Command::FinishRecord).unwrap();
    let document = h.document().unwrap();
    assert_eq!(document.tempo_milli_bpm, 60_000);
    let clip = document.session.slots[0].clip.as_ref().unwrap();
    assert_eq!(clip.length, BeatTime(1_920_000));
    let notes = clip.notes().unwrap();
    assert_eq!(notes[0].duration, BeatTime(1_440_000));
    assert_eq!(notes[0].expression.points[1].offset, BeatTime(480_000));
    assert_eq!(notes[0].expression.points[1].value.pitch, 750_000);
    let reopened = TestHost::from_document(document, Geometry::Pads200).unwrap();
    assert_eq!(reopened.snapshot().tempo_milli_bpm, 60_000);
    assert_eq!(reopened.engine.snapshot().active_voices, 0);
}

#[test]
fn queued_launch_and_held_clip_release_keep_beats_when_wall_time_changes() {
    let mut h = TestHost::new(Session::default(), Geometry::Pads200).unwrap();
    h.command(Command::Generate {
        slot: 0,
        settings: PatternSettings {
            steps: 2,
            pulses: 2,
            step: BeatTime(960_000),
            gate: BeatTime(480_000),
            tone_min: 0,
            tone_max: 0,
            ..PatternSettings::default()
        },
    })
    .unwrap();
    h.advance_micros(250_000).unwrap();
    h.launch_quantum = BeatTime(960_000);
    h.launch(0).unwrap();
    h.set_tempo(250_000, 240_000).unwrap();
    assert_eq!(
        h.engine.slot_launch_time(0).unwrap(),
        Some(BeatTime(960_000))
    );
    h.advance_micros(374_999).unwrap();
    assert_eq!(h.output_count(), 0);
    h.advance_micros(375_000).unwrap();
    assert_eq!(h.engine.snapshot().active_voices, 1);
    let count = h.output_count();
    h.set_tempo(375_000, 60_000).unwrap();
    assert_eq!(h.output_count(), count);
    h.advance_micros(874_999).unwrap();
    assert_eq!(h.engine.snapshot().active_voices, 1);
    h.advance_micros(875_000).unwrap();
    assert_eq!(h.engine.snapshot().active_voices, 0);
    assert_eq!(h.capture.last().unwrap().at, BeatTime(1_440_000));
    h.advance_micros(1_375_000).unwrap();
    assert_eq!(h.engine.snapshot().active_voices, 1);
    h.command(Command::Stop).unwrap();
}

#[test]
fn rejected_tempo_preserves_time_capture_and_session() {
    let mut h = TestHost::new(Session::default(), Geometry::Pads200).unwrap();
    midi(&mut h, 1000, &[0x91, 60, 90]);
    let before = h.document().unwrap();
    let position = h.engine.position();
    let count = h.output_count();
    for (micros, tempo) in [(2000, 0), (2000, 999_001), (999, 90_000)] {
        assert!(h.set_tempo(micros, tempo).is_err());
        assert_eq!(h.document().unwrap(), before);
        assert_eq!(h.engine.position(), position);
        assert_eq!(h.output_count(), count);
    }
    h.disconnect().unwrap();
}

#[test]
fn native_timestamp_order_is_stricter_than_rounded_beat_order() {
    let mut h = TestHost::new(Session::default(), Geometry::Pads200).unwrap();
    h.set_tempo(0, 1_000).unwrap();
    h.advance_micros(60).unwrap();
    assert_eq!(h.engine.position(), BeatTime(0));
    let before = h.document().unwrap();
    assert!(matches!(h.set_tempo(59, 90_000), Err(Error::TimeOrder)));
    assert!(matches!(h.advance_micros(59), Err(Error::TimeOrder)));
    assert!(matches!(
        h.accept_bytes(59, 0x91, Geometry::Pads200),
        Err(Error::TimeOrder)
    ));
    assert_eq!(h.document().unwrap(), before);
    assert_eq!(h.output_count(), 0);
    // Rejected input never reached the parser; data bytes cannot start a note.
    midi(&mut h, 60, &[60, 90]);
    assert_eq!(h.engine.snapshot().active_voices, 0);
    h.set_tempo(60, 120_000).unwrap();
    h.advance_micros(61).unwrap();
    assert_eq!(h.engine.position(), BeatTime(2)); // retained 0.96 + new 1.92 ticks
}

#[test]
fn old_host_envelope_migrates_without_rewriting_source_and_new_tempo_round_trips() {
    let directory = std::env::temp_dir().join(format!("crest-tempo-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("old.json");
    let document = HostDocument::from_session(Session::default()).unwrap();
    let mut old = serde_json::to_value(&document).unwrap();
    old["host_version"] = 1.into();
    old.as_object_mut().unwrap().remove("tempo_milli_bpm");
    let bytes = serde_json::to_vec(&old).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let mut loaded = storage::load_document(&path).unwrap();
    assert_eq!(loaded, document);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    loaded.tempo_milli_bpm = 95_333;
    let new_path = directory.join("new.json");
    storage::save_document(&new_path, &loaded).unwrap();
    assert_eq!(storage::load_document(&new_path).unwrap(), loaded);
    assert_eq!(loaded.host_version, HOST_VERSION);
    let saved = std::fs::read(&new_path).unwrap();
    loaded.tempo_milli_bpm = 0;
    assert!(storage::save_document(&new_path, &loaded).is_err());
    assert_eq!(std::fs::read(&new_path).unwrap(), saved);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn incomplete_mislabeled_and_unknown_host_documents_reject() {
    let document = HostDocument::from_session(Session::default()).unwrap();
    let value = serde_json::to_value(&document).unwrap();
    for version in [0, 1, HOST_VERSION + 1] {
        let mut invalid = value.clone();
        invalid["host_version"] = version.into();
        assert!(serde_json::from_value::<HostDocument>(invalid).is_err());
    }
    let mut missing = value.clone();
    missing.as_object_mut().unwrap().remove("tempo_milli_bpm");
    assert!(serde_json::from_value::<HostDocument>(missing).is_err());
    for tempo in [0, 999, 999_001] {
        let mut invalid = value.clone();
        invalid["tempo_milli_bpm"] = tempo.into();
        assert!(serde_json::from_value::<HostDocument>(invalid).is_err());
    }
}
