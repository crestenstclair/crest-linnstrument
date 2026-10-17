//! Provisional processing-only benchmark, not an audio or Steam Deck latency claim.
use crest_linnstrument::{MpeOutput, MpeZone};
use crest_performance_core::*;
use std::time::Instant;
fn main() -> Result<()> {
    let mut engine = Engine::new(API_VERSION, Session::default(), Limits::default())?;
    let mut output = MpeOutput::new(DestinationId(1), MpeZone::lower());
    for id in 1..=3 {
        engine.input(InputEvent {
            at: BeatTime(0),
            note: key(id),
            gesture: Gesture::On {
                key: 60 + id as u8,
                velocity: 100,
                expression: Expression::default(),
            },
        })?;
    }
    for event in engine.drain_output() {
        std::hint::black_box(output.accept(std::hint::black_box(event))?);
    }
    let mut costs = Vec::with_capacity(10000);
    for frame in 1..=10000 {
        let start = Instant::now();
        let at = BeatTime(frame * 4800);
        engine.advance(at)?;
        for id in 1..=3 {
            engine.input(InputEvent {
                at,
                note: key(id),
                gesture: Gesture::Expression {
                    expression: Expression {
                        pitch: (frame % 100) as i32 * 10000,
                        pressure: (frame % 65536) as u16,
                        timbre: (frame * 7 % 65536) as u16,
                    },
                },
            })?;
        }
        for event in engine.drain_output() {
            std::hint::black_box(output.accept(std::hint::black_box(event))?);
        }
        costs.push(start.elapsed().as_nanos());
    }
    costs.sort_unstable();
    println!(
        "{} {}: 3 simultaneous X/Y/Z voices per frame; processing-only nanoseconds: p50={}, p95={}, p99={}, max={}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        costs[5000],
        costs[9500],
        costs[9900],
        costs[9999]
    );
    Ok(())
}
fn key(note: u64) -> NoteKey {
    NoteKey {
        source: SourceId(1),
        track: TrackId(1),
        note: NoteId(note),
    }
}
