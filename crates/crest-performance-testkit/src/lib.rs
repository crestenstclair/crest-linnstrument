//! Deterministic public-API fixtures. Protocol fixtures are not firmware proof.
use crest_linnstrument::*;
use crest_performance_core::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapturedMidi {
    pub at: BeatTime,
    pub destination: DestinationId,
    pub message: MidiMessage,
}
#[derive(Default)]
pub struct CaptureSink {
    pub events: Vec<OutputEvent>,
    pub midi: Vec<CapturedMidi>,
}
impl CaptureSink {
    pub fn collect(&mut self, engine: &mut Engine, output: &mut MpeOutput) -> Result<()> {
        for event in engine.drain_output() {
            let batch = output.accept(event)?;
            self.midi
                .extend(batch.messages().iter().map(|message| CapturedMidi {
                    at: event.at,
                    destination: event.destination,
                    message: *message,
                }));
            self.events.push(event);
        }
        Ok(())
    }
}
/// Explicit protocol-double fixture: three simultaneous notes with independent
/// X/Y/Z. The firmware acceptance suite must independently produce these classes
/// of behavior from contacts; this fixture never claims sensor-path coverage.
pub fn expressive_phrase(
    engine: &mut Engine,
    sink: &mut CaptureSink,
    output: &mut MpeOutput,
) -> Result<()> {
    let mut input = NoteInput::mpe(MpeZone::lower(), SourceId(1), TrackId(1));
    let mut parser = MidiParser::default();
    for (tick, bytes) in [
        (0, [0x91, 60, 100]),
        (0, [0x92, 64, 90]),
        (0, [0x93, 67, 80]),
        (240_001, [0xe1, 0, 65]),
        (250_003, [0xb2, 74, 110]),
        (260_007, [0xd3, 90, 0xf8]),
        (720_009, [0x81, 60, 0]),
        (730_011, [0x82, 64, 0]),
        (740_013, [0x83, 67, 0]),
    ] {
        feed(engine, &mut input, &mut parser, BeatTime(tick), &bytes)?;
        sink.collect(engine, output)?;
    }
    Ok(())
}
pub fn feed(
    engine: &mut Engine,
    input: &mut NoteInput,
    parser: &mut MidiParser,
    at: BeatTime,
    bytes: &[u8],
) -> Result<()> {
    engine.advance(at)?;
    for byte in bytes {
        if let Some(message) = parser.feed(*byte) {
            for event in input.accept(at, message)? {
                engine.input(*event)?;
            }
        }
    }
    Ok(())
}
