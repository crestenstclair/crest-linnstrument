//! Descriptions track real public commands, independently of displayed LED frames.
use crest_linnstrument::Geometry;
use crest_performance_core::*;
use crest_performance_host::{PadHelp, TestHost};

fn help(host: &TestHost, column: u8, row: u8) -> PadHelp {
    host.snapshot()
        .pad_help
        .into_iter()
        .find(|pad| pad.column == column && pad.row == row)
        .unwrap()
}

#[test]
fn descriptions_follow_slot_lifecycle_and_actual_control_availability() {
    for geometry in [Geometry::Pads128, Geometry::Pads200] {
        let mut host = TestHost::new(Session::default(), geometry).unwrap();
        assert!(host.snapshot().pad_help.is_empty());
        host.command(Command::SetMode(SurfaceMode::Clips)).unwrap();
        assert!(help(&host, 1, 0).action.contains("Empty"));
        assert!(help(&host, 1, 1).action.contains("Unavailable"));
        host.command(Command::Generate {
            slot: 0,
            settings: PatternSettings::default(),
        })
        .unwrap();
        assert!(help(&host, 1, 0).action.contains("Recorded"));
        host.launch(0).unwrap();
        assert!(help(&host, 1, 0).action.contains("Playing"));
        host.command(Command::SetClipMuted {
            slot: 0,
            muted: true,
        })
        .unwrap();
        assert!(help(&host, 1, 0).action.contains("Muted"));
        host.command(Command::Stop).unwrap();
        assert!(help(&host, 1, 0).action.contains("Recorded"));
        assert!(help(&host, 1, 4).action.contains("Unavailable"));
        host.command(Command::SetHarmony(HarmonySettings {
            tonic: 120,
            ..HarmonySettings::default()
        }))
        .unwrap();
        assert!(help(&host, 3, 6).action.contains("Unavailable"));
        assert!(
            host.snapshot()
                .pad_help
                .iter()
                .all(|pad| pad.column < geometry.columns() && pad.row < 8)
        );
        assert!(help(&host, 1, 2).action.contains("inactive"));
        host.command(Command::SetMode(SurfaceMode::Play)).unwrap();
        assert!(host.snapshot().pad_help.is_empty());
    }
}
