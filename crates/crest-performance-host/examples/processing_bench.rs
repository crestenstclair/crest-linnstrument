//! Report processing cost separately from sensing, MIDI transport and audio.
//! Run with --release. This is a provisional software workload, not Deck qualification.
use crest_linnstrument::{MpeOutput, MpeZone};
use crest_performance_core::*;
use serde::Serialize;
use std::{hint::black_box, time::Instant};

const NOTES: u64 = 6;
const FRAME_TICKS: u64 = TICKS_PER_BEAT / 50; // 10 ms at 120 BPM.
const LOOP_FRAMES: u64 = 200;
const RUN_FRAMES: u64 = 18_000; // Three virtual minutes; rolls bounded history.

fn note(source: u32, index: u64) -> NoteKey {
    NoteKey {
        source: SourceId(source),
        track: TrackId(1),
        note: NoteId(index + 1),
    }
}
fn expression(frame: u64, index: u64) -> Expression {
    Expression {
        pitch: ((frame * 137 + index * 13) % 2_000) as i32 * 1000 - 1_000_000,
        pressure: ((frame * 173 + index * 101) % 65_536) as u16,
        timbre: ((frame * 211 + index * 79) % 65_536) as u16,
    }
}
fn input_frame(engine: &mut Engine, frame: u64, chord_triggers: bool) -> Result<()> {
    let at = BeatTime(frame * FRAME_TICKS);
    engine.advance(at)?;
    if chord_triggers && frame % 100 > 80 {
        return Ok(());
    }
    for index in 0..if chord_triggers { 3 } else { NOTES } {
        let expression = expression(frame, index);
        let gesture = if chord_triggers && frame % 100 == 80 {
            Gesture::Off { velocity: 64 }
        } else if frame == 0 || (chord_triggers && frame.is_multiple_of(100)) {
            Gesture::On {
                key: if chord_triggers { 30 } else { 60 } + index as u8,
                velocity: 100,
                expression,
            }
        } else {
            Gesture::Expression { expression }
        };
        engine.input(InputEvent {
            at,
            note: note(
                2,
                if chord_triggers {
                    frame / 100 * 3 + index
                } else {
                    index
                },
            ),
            gesture,
        })?;
    }
    Ok(())
}
fn clip_frame(frame: u64) -> impl Iterator<Item = InputEvent> {
    (0..NOTES).map(move |index| InputEvent {
        at: BeatTime(frame * FRAME_TICKS),
        note: note(1, index),
        gesture: if frame == 0 {
            Gesture::On {
                key: 60 + index as u8,
                velocity: 100,
                expression: expression(frame, index),
            }
        } else if frame == LOOP_FRAMES {
            Gesture::Off { velocity: 64 }
        } else {
            Gesture::Expression {
                expression: expression(frame, index),
            }
        },
    })
}
fn session() -> Session {
    let mut session = Session::default();
    session.slots[0].clip = Some(Clip {
        length: BeatTime(LOOP_FRAMES * FRAME_TICKS),
        events: (0..=LOOP_FRAMES).flat_map(clip_frame).collect(),
        harmony_origin: Vec::new(),
        pattern_origin: Vec::new(),
        follow: None,
        conditions: Vec::new(),
    });
    session
}

#[derive(Default)]
struct Capture {
    events: u64,
    midi_messages: u64,
    attacks: u64,
    releases: u64,
    members: [Option<u8>; 16],
}
impl Capture {
    fn collect(&mut self, engine: &mut Engine, output: &mut MpeOutput) -> Result<()> {
        for event in engine.drain_output() {
            self.events += 1;
            self.messages(output.accept(event)?)?;
        }
        Ok(())
    }
    fn messages(&mut self, batch: crest_linnstrument::MidiBatch) -> Result<()> {
        for message in batch.messages() {
            let [status, key, _] = black_box(message.bytes());
            self.midi_messages += 1;
            let channel = usize::from(status & 15);
            match status & 0xf0 {
                0x90 => {
                    if channel == 0 || self.members[channel].replace(key).is_some() {
                        return Err(Error::Invalid("benchmark MPE channel collision"));
                    }
                    self.attacks += 1;
                }
                0x80 => {
                    if self.members[channel].take() != Some(key) {
                        return Err(Error::Invalid("benchmark mismatched release"));
                    }
                    self.releases += 1;
                }
                _ => {}
            }
        }
        Ok(())
    }
}
#[derive(Serialize)]
struct Report {
    package: &'static str,
    os: &'static str,
    arch: &'static str,
    debug_assertions: bool,
    workload: &'static str,
    virtual_seconds: u64,
    measured_batches: usize,
    events: u64,
    midi_messages: u64,
    attacks: u64,
    releases: u64,
    p50_ns: u128,
    p95_ns: u128,
    p99_ns: u128,
    max_ns: u128,
    batches_over_1ms: usize,
}
fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let articulation = match args.as_slice() {
        [] => None,
        [arg] if arg == "--strum" => Some(Articulation::Strum {
            direction: StrumDirection::Ascending,
            interval: BeatTime(FRAME_TICKS * 3),
        }),
        [arg] if arg == "--arpeggio" => Some(Articulation::Arpeggio {
            direction: StrumDirection::Ascending,
            interval: BeatTime(FRAME_TICKS * 3),
            gate: BeatTime(FRAME_TICKS * 2),
        }),
        [arg] if arg == "--humanized" => Some(Articulation::Humanized {
            max_delay: BeatTime(60000),
            velocity_variation: 8,
            seed: 42,
        }),
        _ => return Err("usage: processing_bench [--strum | --arpeggio | --humanized]".into()),
    };
    let mut engine = Engine::new(
        API_VERSION,
        session(),
        Limits {
            voices: 15,
            ..Limits::default()
        },
    )?;
    let mut output = MpeOutput::new(DestinationId(1), MpeZone::lower());
    let mut capture = Capture::default();
    let mut samples = Vec::with_capacity(RUN_FRAMES as usize);
    if let Some(articulation) = articulation {
        engine.command(Command::SetHarmony(HarmonySettings {
            articulation,
            ..HarmonySettings::default()
        }))?;
        engine.command(Command::SetMode(SurfaceMode::Chords))?;
    }
    engine.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(0),
    })?;
    for frame in 0..RUN_FRAMES {
        let started = Instant::now();
        input_frame(&mut engine, frame, articulation.is_some())?;
        capture.collect(&mut engine, &mut output)?;
        let elapsed = started.elapsed().as_nanos();
        if frame >= 100 {
            samples.push(elapsed);
        } // One virtual second of warmup.
    }
    engine.command(Command::Stop)?;
    capture.collect(&mut engine, &mut output)?;
    assert_eq!(capture.attacks, capture.releases);
    assert!(capture.members.iter().all(Option::is_none));
    assert_eq!(engine.snapshot().active_voices, 0);
    samples.sort_unstable();
    let percentile = |percent: usize| samples[(samples.len() * percent).div_ceil(100) - 1];
    let report = Report {
        package: env!("CARGO_PKG_VERSION"),
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        debug_assertions: cfg!(debug_assertions),
        workload: if articulation.is_some_and(Articulation::is_repeating) {
            "3 expressive arpeggio triggers (9 reserved voices) + 6 looping notes, 30 ms steps/20 ms gates, XYZ at 100 Hz; advance/input/MPE allocation/capture validation; no UI, files, sensor, OS MIDI or audio in timed batches"
        } else if articulation.is_some_and(Articulation::is_humanized) {
            "3 repeating humanized chord triggers (9 reserved voices) + 6 looping notes, 31.25 ms maximum delay, velocity ±8, XYZ at 100 Hz; advance/input/MPE allocation/capture validation; no UI, files, sensor, OS MIDI or audio in timed batches"
        } else if articulation.is_some() {
            "3 repeating expressive strum triggers (9 voices) + 6 looping notes, XYZ at 100 Hz; advance/input/MPE allocation/capture validation; no UI, files, sensor, OS MIDI or audio in timed batches"
        } else {
            "6 live + 6 looping notes, overlapping pitches, XYZ at 100 Hz; advance/input/MPE allocation/capture validation; no UI, files, sensor, OS MIDI or audio in timed batches"
        },
        virtual_seconds: RUN_FRAMES / 100,
        measured_batches: samples.len(),
        events: capture.events,
        midi_messages: capture.midi_messages,
        attacks: capture.attacks,
        releases: capture.releases,
        p50_ns: percentile(50),
        p95_ns: percentile(95),
        p99_ns: percentile(99),
        max_ns: *samples.last().unwrap(),
        batches_over_1ms: samples.iter().filter(|&&n| n >= 1_000_000).count(),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}
