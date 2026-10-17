//! Protocol fixtures for the shared host boundary, not firmware validation.
use crest_linnstrument::Geometry;
use crest_performance_core::*;
use crest_performance_host::TestHost;

fn pad(host: &mut TestHost, micros: u64, bytes: [u8; 3]) -> Result<()> {
    for byte in bytes {
        host.accept_bytes(micros, byte, Geometry::Pads128)?;
    }
    Ok(())
}
fn looping_host() -> TestHost {
    let mut host = TestHost::new(Session::default(), Geometry::Pads128).unwrap();
    for (slot, follow_harmony) in [(0, true), (1, false)] {
        host.command(Command::Generate {
            slot,
            settings: PatternSettings {
                steps: 2,
                pulses: 2,
                step: BeatTime(TICKS_PER_BEAT),
                gate: BeatTime(TICKS_PER_BEAT * 3 / 4),
                tone_min: 0,
                tone_max: 0,
                follow_harmony,
                ..PatternSettings::default()
            },
        })
        .unwrap();
    }
    host.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
    pad(&mut host, 0, [0x90, 1, 90]).unwrap();
    pad(&mut host, 0, [0x90, 2, 90]).unwrap();
    host.advance_micros(0).unwrap();
    host
}
fn notes(host: &TestHost, status: u8) -> Vec<(u64, u8)> {
    host.capture
        .iter()
        .filter(|message| message.message.bytes()[0] & 0xf0 == status)
        .map(|message| (message.at.0, message.message.bytes()[1]))
        .collect()
}

#[test]
fn top_row_takes_over_harmony_without_releasing_held_notes_or_changing_fixed_clips() {
    let mut host = looping_host();
    let saved = host.document().unwrap();
    host.command(Command::SetHarmonySource(HarmonySource::Clip { slot: 1 }))
        .unwrap();
    assert_eq!(notes(&host, 0x90), [(0, 60), (0, 60)]);
    let before = host.output_count();
    pad(&mut host, 250_000, [0x97, 5, 90]).unwrap(); // top-row V
    let state = host.engine.snapshot();
    assert_eq!(state.harmony.degree, 4);
    assert_eq!(state.harmony_source, HarmonySource::Selected {});
    assert_eq!(state.active_voices, 2);
    assert_eq!(&state.slots[..2], &[SlotState::Playing; 2]);
    assert_eq!(host.output_count(), before, "selection emits no note data");
    host.advance_micros(500_000).unwrap();
    assert_eq!(notes(&host, 0x80), [(720_000, 60), (720_000, 60)]);
    assert_eq!(
        notes(&host, 0x90),
        [(0, 60), (0, 60), (960_000, 67), (960_000, 60)]
    );
    let changed = host.document().unwrap();
    assert_eq!(changed.session.slots, saved.session.slots);
    let reopened = TestHost::from_document(changed, Geometry::Pads128).unwrap();
    assert_eq!(reopened.engine.snapshot().harmony.degree, 4);
    assert_eq!(reopened.engine.snapshot().active_voices, 0);
    host.command(Command::Stop).unwrap();
    assert_eq!(notes(&host, 0x90).len(), notes(&host, 0x80).len());
}

#[test]
fn releases_control_column_and_unused_pads_never_select_harmony_or_launch_clips() {
    let mut host = looping_host();
    pad(&mut host, 0, [0x97, 7, 90]).unwrap();
    assert_eq!(host.engine.snapshot().harmony.degree, 6);
    host.command(Command::SetHarmony(HarmonySettings {
        degree: 1,
        ..HarmonySettings::default()
    }))
    .unwrap();
    let before = host.document().unwrap();
    for bytes in [
        [0x87, 7, 5], // slide release velocity is not an attack
        [0x97, 7, 0],
        [0x97, 0, 90],
        [0x97, 8, 90],
        [0x96, 5, 90],
        [0x90, 9, 90],
    ] {
        pad(&mut host, 0, bytes).unwrap();
    }
    assert_eq!(host.document().unwrap(), before);
    assert_eq!(&host.engine.snapshot().slots[..2], &[SlotState::Playing; 2]);
    host.command(Command::Stop).unwrap();
    host.command(Command::SetMode(SurfaceMode::Play)).unwrap();
    pad(&mut host, 0, [0x97, 5, 90]).unwrap();
    assert_eq!(host.engine.snapshot().harmony.degree, 1);
    assert_eq!(notes(&host, 0x90).last(), Some(&(0, 5)));
    pad(&mut host, 0, [0x87, 5, 0]).unwrap();
    assert_eq!(host.engine.snapshot().active_voices, 0);
}

#[test]
fn invalid_degree_preserves_selected_harmony_and_clip_source() {
    let mut session = Session::default();
    session.harmony.tonic = 120; // I fits; VII would exceed MIDI 127.
    session.harmony_source = HarmonySource::Clip { slot: 0 };
    let mut host = TestHost::new(session, Geometry::Pads128).unwrap();
    host.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
    let before = host.document().unwrap();
    assert!(pad(&mut host, 0, [0x97, 7, 90]).is_err());
    assert_eq!(host.document().unwrap(), before);
    assert_eq!(host.output_count(), 0);
}
