use crest_linnstrument::*;
use crest_performance_core::*;
use crest_performance_host::storage::{load_session, save_session};
use crest_performance_testkit::*;
use std::{error::Error, path::PathBuf};

fn main() -> std::result::Result<(), Box<dyn Error>> {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "demo".into());
    #[cfg(feature = "midi")]
    match command.as_str() {
        "ports" => return crest_performance_host::midi_runner::print_ports().map_err(Into::into),
        "midi" => {
            let input = args
                .next()
                .ok_or("midi INPUT_ID SOUND_OUTPUT_ID|- FEEDBACK_OUTPUT_ID|-")?;
            let sound = args.next().unwrap_or_else(|| "-".into());
            let feedback = args.next().unwrap_or_else(|| "-".into());
            return crest_performance_host::midi_runner::run(&input, &sound, &feedback)
                .map_err(Into::into);
        }
        _ => {}
    }
    let path = PathBuf::from(
        args.next()
            .unwrap_or_else(|| "target/demo-session.json".into()),
    );
    match command.as_str() {
        "demo" => demo(path),
        "inspect" => {
            let session = load_session(&path)?;
            let engine = Engine::new(API_VERSION, session, Limits::default())?;
            println!("{}", serde_json::to_string_pretty(&engine.snapshot())?);
            Ok(())
        }
        _ => Err("usage: crest-performance-host [demo|inspect] [session.json]".into()),
    }
}
fn demo(path: PathBuf) -> std::result::Result<(), Box<dyn Error>> {
    let mut engine = Engine::new(API_VERSION, Session::default(), Limits::default())?;
    let mut output = MpeOutput::new(DestinationId(1), MpeZone::lower());
    let mut sink = CaptureSink::default();
    engine.command(Command::BeginRecord { slot: 0 })?;
    expressive_phrase(&mut engine, &mut sink, &mut output)?;
    engine.advance(BeatTime(TICKS_PER_BEAT))?;
    engine.command(Command::FinishRecord)?;
    let session = engine.save_session()?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    save_session(&path, &session)?;
    let reopened = load_session(&path)?;
    let mut engine = Engine::new(API_VERSION, reopened, Limits::default())?;
    engine.command(Command::Launch {
        slot: 0,
        quantum: BeatTime(0),
    })?;
    engine.advance(BeatTime(TICKS_PER_BEAT * 2 - 1))?;
    sink.collect(&mut engine, &mut output)?;
    engine.command(Command::Stop)?;
    sink.collect(&mut engine, &mut output)?;
    println!("{}", serde_json::to_string_pretty(&sink.events)?);
    eprintln!(
        "Protocol fixture only: saved {}, replayed two loops; {} events, {} MIDI messages. Firmware acceptance is separate.",
        path.display(),
        sink.events.len(),
        sink.midi.len()
    );
    Ok(())
}
