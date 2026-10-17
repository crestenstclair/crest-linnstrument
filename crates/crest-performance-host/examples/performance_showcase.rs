//! Reproducible demonstration bank, built entirely through public library APIs.
//! The source phrase is an existing firmware capture; this builder is not a
//! substitute for live sensor/firmware acceptance.
use crest_linnstrument::Geometry;
use crest_performance_core::*;
use crest_performance_host::{TestHost, storage};
use std::{collections::BTreeMap, path::PathBuf};

fn session() -> Result<Session> {
    let source: Session =
        serde_json::from_str(include_str!("../../../examples/expressive-phrase.json"))
            .map_err(|_| Error::Invalid("bundled source phrase"))?;
    let phrase = source.migrate()?.slots[0]
        .clip
        .clone()
        .ok_or(Error::NotFound)?;
    let mut session = Session::default();
    session.tracks[0].name = "Expressive chords".into();
    session.tracks[0].instrument.reference = "capture://chords".into();
    session.tracks.extend([
        Track {
            id: TrackId(2),
            name: "Following bass".into(),
            instrument: InstrumentTarget {
                destination: DestinationId(2),
                reference: "capture://bass".into(),
            },
        },
        Track {
            id: TrackId(3),
            name: "Fill accompaniment".into(),
            instrument: InstrumentTarget {
                destination: DestinationId(3),
                reference: "capture://fill".into(),
            },
        },
    ]);
    session.slots[1].track = TrackId(2);
    session.slots[2].track = TrackId(3);
    let mut e = Engine::new(API_VERSION, session, Limits::default())?;
    e.command(Command::SetHarmony(HarmonySettings {
        articulation: Articulation::Humanized {
            max_delay: BeatTime(60000),
            velocity_variation: 8,
            seed: 42,
        },
        bass_track: Some(TrackId(2)),
        ..HarmonySettings::default()
    }))?;
    e.command(Command::SetMode(SurfaceMode::Chords))?;
    e.command(Command::BeginRecord { slot: 0 })?;
    for input in phrase.events {
        e.advance(input.at)?;
        e.input(input)?;
    }
    e.advance(phrase.length)?;
    e.command(Command::FinishRecord)?;
    e.command(Command::Generate {
        slot: 1,
        settings: PatternSettings {
            steps: 8,
            pulses: 3,
            pitch_steps: 1,
            tone_min: -3,
            tone_max: -3,
            seed: 7,
            follow_harmony: true,
            ..PatternSettings::default()
        },
    })?;
    e.command(Command::Generate {
        slot: 2,
        settings: PatternSettings {
            steps: 4,
            pulses: 4,
            pitch_steps: 4,
            seed: 17,
            velocity: 72,
            velocity_variation: 8,
            timing_variation: BeatTime(12000),
            follow_harmony: true,
            ..PatternSettings::default()
        },
    })?;
    let fill_notes = e.session().slots[2]
        .clip
        .as_ref()
        .ok_or(Error::NotFound)?
        .notes()?;
    for note in fill_notes {
        e.command(Command::SetNoteCondition {
            slot: 2,
            note: note.id,
            condition: NoteCondition::Fill {},
        })?;
    }
    e.command(Command::SetHarmonySource(HarmonySource::Clip { slot: 0 }))?;
    e.save_session()
}

fn verify(document: storage::HostDocument) -> Result<()> {
    assert_eq!(
        document.session.slots[0]
            .clip
            .as_ref()
            .unwrap()
            .notes()?
            .len(),
        9
    );
    let mut host = TestHost::from_document(document, Geometry::Pads200)?;
    for slot in 0..3 {
        host.launch(slot)?;
    }
    host.advance_micros(4_000_000)?;
    assert!(
        host.capture
            .iter()
            .any(|m| m.destination == DestinationId(1) && m.message.bytes()[0] & 0xf0 == 0x90)
    );
    assert!(
        host.capture
            .iter()
            .any(|m| m.destination == DestinationId(2) && m.message.bytes()[0] & 0xf0 == 0x90)
    );
    assert!(
        !host
            .capture
            .iter()
            .any(|m| m.destination == DestinationId(3))
    );
    host.command(Command::SetFill(true))?;
    host.advance_micros(5_000_000)?;
    assert!(
        host.capture
            .iter()
            .any(|m| m.destination == DestinationId(3) && m.message.bytes()[0] & 0xf0 == 0x90)
    );
    host.command(Command::SetFill(false))?;
    host.advance_micros(6_000_000)?;
    host.command(Command::Stop)?;
    let mut held = BTreeMap::new();
    let mut attacks = 0;
    for message in &host.capture {
        let bytes = message.message.bytes();
        let member = (message.destination, bytes[0] & 15);
        match bytes[0] & 0xf0 {
            0x90 => {
                assert_ne!(member.1, 0);
                assert!(held.insert(member, bytes[1]).is_none());
                attacks += 1;
            }
            0x80 => assert_eq!(held.remove(&member), Some(bytes[1])),
            _ => {}
        }
    }
    assert!(held.is_empty());
    assert_eq!(host.engine.snapshot().active_voices, 0);
    println!(
        "Showcase verified: three destinations, {attacks} balanced attacks/releases, Fill silent until held."
    );
    Ok(())
}

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let path = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "target/performance-showcase.json".into()),
    );
    let host = TestHost::new(session()?, Geometry::Pads200)?;
    let document = host.document()?;
    verify(document.clone())?;
    storage::save_document(&path, &document)?;
    assert_eq!(storage::load_document(&path)?, document);
    println!("Saved {}", path.display());
    Ok(())
}
