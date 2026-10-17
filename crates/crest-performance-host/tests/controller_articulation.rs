//! Protocol inputs; separate native tests cover the real sensor/firmware path.
use crest_linnstrument::Geometry;
use crest_performance_core::*;
use crest_performance_host::TestHost;

fn pad(host: &mut TestHost, column: u8, pressed: bool) {
    for byte in [if pressed { 0x95 } else { 0x85 }, column, 90] {
        host.accept_bytes(0, byte, Geometry::Pads128).unwrap();
    }
}
fn tap(host: &mut TestHost, column: u8) {
    pad(host, column, true);
    pad(host, column, false);
}

#[test]
fn controller_articulations_select_exact_presets_on_learned_chords_and_persist() {
    let mut session = Session::default();
    session.harmony.learned[0] = Some(LearnedChord::new(&[60, 67, 72], 60).unwrap());
    session.harmony_source = HarmonySource::Clip { slot: 0 };
    let mut host = TestHost::new(session.clone(), Geometry::Pads128).unwrap();
    host.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
    for (column, expected) in [
        (
            7,
            Articulation::Strum {
                direction: StrumDirection::Ascending,
                interval: BeatTime(60_000),
            },
        ),
        (
            8,
            Articulation::Strum {
                direction: StrumDirection::Descending,
                interval: BeatTime(60_000),
            },
        ),
        (
            9,
            Articulation::Arpeggio {
                direction: StrumDirection::Ascending,
                interval: BeatTime(60_000),
                gate: BeatTime(30_000),
            },
        ),
        (
            10,
            Articulation::Arpeggio {
                direction: StrumDirection::Descending,
                interval: BeatTime(60_000),
                gate: BeatTime(30_000),
            },
        ),
        (
            11,
            Articulation::Humanized {
                max_delay: BeatTime(60_000),
                velocity_variation: 8,
                seed: 42,
            },
        ),
        (6, Articulation::Block),
    ] {
        tap(&mut host, column);
        assert_eq!(host.engine.snapshot().harmony.articulation, expected);
        assert_eq!(
            host.engine.snapshot().harmony.learned,
            session.harmony.learned
        );
        assert_eq!(
            host.engine.session().harmony_source,
            HarmonySource::Selected {}
        );
        let document = host.document().unwrap();
        let reopened = TestHost::from_document(document.clone(), Geometry::Pads128).unwrap();
        assert_eq!(reopened.document().unwrap(), document);
        assert_eq!(host.output_count(), 0, "selection cannot synthesize notes");
    }
}

#[test]
fn direction_and_reselection_keep_custom_timing_and_duplicate_presses_cannot_take_over() {
    let mut host = TestHost::new(Session::default(), Geometry::Pads200).unwrap();
    host.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
    for (initial, column, expected) in [
        (
            Articulation::Strum {
                direction: StrumDirection::Ascending,
                interval: BeatTime(33_333),
            },
            8,
            Articulation::Strum {
                direction: StrumDirection::Descending,
                interval: BeatTime(33_333),
            },
        ),
        (
            Articulation::Arpeggio {
                direction: StrumDirection::Descending,
                interval: BeatTime(77_777),
                gate: BeatTime(22_222),
            },
            9,
            Articulation::Arpeggio {
                direction: StrumDirection::Ascending,
                interval: BeatTime(77_777),
                gate: BeatTime(22_222),
            },
        ),
        (
            Articulation::Humanized {
                max_delay: BeatTime(91_917),
                velocity_variation: 37,
                seed: 91,
            },
            11,
            Articulation::Humanized {
                max_delay: BeatTime(91_917),
                velocity_variation: 37,
                seed: 91,
            },
        ),
    ] {
        host.command(Command::SetHarmony(HarmonySettings {
            articulation: initial,
            ..HarmonySettings::default()
        }))
        .unwrap();
        pad(&mut host, column, true);
        assert_eq!(host.engine.snapshot().harmony.articulation, expected);
        tap(&mut host, 6); // Another finger selects Block while the first stays down.
        pad(&mut host, column, true);
        assert_eq!(
            host.engine.snapshot().harmony.articulation,
            Articulation::Block
        );
        pad(&mut host, column, false);
        tap(&mut host, column);
        assert_ne!(
            host.engine.snapshot().harmony.articulation,
            Articulation::Block
        );
    }
    assert_eq!(host.engine.snapshot().active_voices, 0);
}
