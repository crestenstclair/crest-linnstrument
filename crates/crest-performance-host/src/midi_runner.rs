//! Small interactive host for OS MIDI devices. No simulator or GUI is required.
use crate::{TestHost, midi_ports::*, storage};
use crest_linnstrument::{Geometry, user_mode};
use crest_performance_core::{BeatTime, Command, DestinationId, Session, SurfaceMode};
use std::{
    io::{self, BufRead},
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub fn print_ports() -> Result<(), String> {
    for port in input_ports()? {
        println!("IN  {}  {}", port.id, port.name);
    }
    for port in output_ports()? {
        println!("OUT {}  {}", port.id, port.name);
    }
    Ok(())
}

/// Each external destination is explicit. There is no MIDI thru or forwarding
/// from the sound sink back into the controller, preventing an implicit loop.
pub fn run(input_id: &str, sound_id: &str, device_id: &str) -> Result<(), String> {
    if sound_id != "-" && sound_id == device_id {
        return Err("sound output and controller feedback must be different ports".into());
    }
    let shutdown = Arc::new(AtomicBool::new(false));
    let signal = Arc::clone(&shutdown);
    ctrlc::set_handler(move || signal.store(true, Ordering::Release)).map_err(|e| e.to_string())?;
    let origin = Instant::now();
    let input = InputPort::connect(input_id, origin)?;
    let mut sound = if sound_id == "-" {
        None
    } else {
        Some(OutputPort::connect(sound_id)?)
    };
    let mut device = if device_id == "-" {
        None
    } else {
        Some(OutputPort::connect(device_id)?)
    };
    let mut host =
        TestHost::new(Session::default(), Geometry::Pads200).map_err(|e| e.to_string())?;
    configure(&host, &mut sound)?;
    if let Some(device) = &mut device {
        for message in user_mode(false) {
            device.send(message.as_bytes())?;
        }
    }
    let (sender, commands) = mpsc::sync_channel(16);
    std::thread::spawn(move || {
        let mut input = io::stdin().lock();
        loop {
            let mut line = String::new();
            let result = std::io::Read::take(&mut input, 4097).read_line(&mut line);
            match result {
                Ok(0) | Err(_) => break,
                Ok(_) if line.len() > 4096 => {
                    let _ = sender.send("quit".into());
                    break;
                }
                Ok(_) => {
                    if sender.send(line).is_err() {
                        break;
                    }
                }
            }
        }
    });
    eprintln!(
        "MPE lower zone, members 2–16, ±48 semitones. Commands: record N, overdub N, finish, cancel, launch N, quantum TICKS, clips, play, chords, harmony JSON, profile JSON, stop, save PATH, open PATH, quit. Slots 1–8. '-' sound port uses capture only."
    );
    let routes = Routes {
        input_id,
        sound_id,
        device_id,
        input: &input,
        sound: &mut sound,
        device: &mut device,
    };
    let result = run_loop(&mut host, routes, &commands, origin, &shutdown);
    // Always attempt releases and restore stock playing, including error paths.
    let mut cursor = host.output_count();
    let _ = host.disconnect();
    let _ = flush(&mut host, &mut sound, &mut device, &mut cursor);
    if let Some(sound) = &mut sound {
        sound.silence();
    }
    if let Some(device) = &mut device {
        for message in user_mode(false) {
            let _ = device.send(message.as_bytes());
        }
    }
    result
}

struct Routes<'a> {
    input_id: &'a str,
    sound_id: &'a str,
    device_id: &'a str,
    input: &'a InputPort,
    sound: &'a mut Option<OutputPort>,
    device: &'a mut Option<OutputPort>,
}
fn run_loop(
    host: &mut TestHost,
    routes: Routes<'_>,
    commands: &mpsc::Receiver<String>,
    origin: Instant,
    shutdown: &AtomicBool,
) -> Result<(), String> {
    let Routes {
        input_id,
        sound_id,
        device_id,
        input,
        sound,
        device,
    } = routes;
    let mut cursor = 0;
    let mut advanced_us = 0;
    let mut accepting_after = 0;
    let mut last_health = Instant::now();
    loop {
        if shutdown.load(Ordering::Acquire) {
            return Ok(());
        }
        // Input is serviced ahead of LEDs; bound each cycle so clock/commands progress.
        for _ in 0..256 {
            let Some(packet) = input.poll()? else {
                break;
            };
            if packet.received_us < accepting_after {
                continue;
            }
            let at = packet.received_us.max(advanced_us);
            for byte in packet.bytes() {
                host.accept_bytes(at, *byte, Geometry::Pads200)
                    .map_err(|e| e.to_string())?;
            }
            advanced_us = at;
            flush(host, sound, device, &mut cursor)?;
        }
        advanced_us = (origin.elapsed().as_micros() as u64).max(advanced_us);
        host.advance_micros(advanced_us)
            .map_err(|e| e.to_string())?;
        flush(host, sound, device, &mut cursor)?;
        match commands.try_recv() {
            Ok(line) => match apply_command(host, line.trim(), advanced_us) {
                Ok(false) => return Ok(()),
                Ok(true) => {
                    if matches!(line.trim(), "clips" | "play" | "chords")
                        || line.trim().starts_with("open ")
                    {
                        accepting_after = advanced_us.saturating_add(150_000);
                    }
                    if line.trim().starts_with("open ") {
                        cursor = 0;
                        configure(host, sound)?;
                    }
                    eprintln!(
                        "{}",
                        serde_json::to_string(&host.engine.snapshot()).map_err(|e| e.to_string())?
                    );
                }
                Err(error) => eprintln!("{error}"),
            },
            Err(mpsc::TryRecvError::Disconnected) => return Ok(()),
            Err(mpsc::TryRecvError::Empty) => {}
        }
        host.paint().map_err(|e| e.to_string())?;
        flush(host, sound, device, &mut cursor)?;
        if last_health.elapsed() >= Duration::from_secs(1) {
            if !input_ports()?.iter().any(|p| p.id == input_id) {
                return Err("MIDI input removed".into());
            }
            let outputs = output_ports()?;
            if [sound_id, device_id]
                .iter()
                .any(|id| *id != "-" && !outputs.iter().any(|p| p.id == *id))
            {
                return Err("MIDI output removed".into());
            }
            last_health = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(1));
    }
}
fn configure(host: &TestHost, sound: &mut Option<OutputPort>) -> Result<(), String> {
    if let Some(output) = sound {
        for message in host.output_configuration() {
            output.send(message.message.as_bytes())?;
        }
    }
    Ok(())
}
fn flush(
    host: &mut TestHost,
    sound: &mut Option<OutputPort>,
    device: &mut Option<OutputPort>,
    cursor: &mut u64,
) -> Result<(), String> {
    for message in host.output_since(*cursor).map_err(|e| e.to_string())? {
        if message.destination != DestinationId(1) {
            return Err("CLI connects only destination 1; session needs another route".into());
        }
        if let Some(output) = sound {
            output.send(message.message.as_bytes())?;
        }
    }
    *cursor = host.output_count();
    // One small ordered LED/configuration batch per cycle, after musical events.
    let messages: Vec<_> = host.device_commands.drain(..).collect();
    if let Some(device) = device {
        for message in messages {
            device.send(message.as_bytes())?;
        }
    }
    Ok(())
}
fn apply_command(host: &mut TestHost, line: &str, micros: u64) -> Result<bool, String> {
    let (verb, argument) = line.split_once(' ').unwrap_or((line, ""));
    let slot = || {
        argument
            .parse::<usize>()
            .ok()
            .and_then(|n| n.checked_sub(1))
            .filter(|n| *n < 8)
            .ok_or("slot must be 1–8".to_string())
    };
    let command = match verb {
        "quit" => return Ok(false),
        "record" => Command::BeginRecord { slot: slot()? },
        "overdub" => Command::BeginOverdub { slot: slot()? },
        "finish" => Command::FinishRecord,
        "cancel" => Command::CancelRecord,
        "launch" => {
            host.launch(slot()?).map_err(|e| e.to_string())?;
            return Ok(true);
        }
        "quantum" => {
            host.launch_quantum = BeatTime(
                argument
                    .parse()
                    .map_err(|_| "quantum requires ticks; 0 is immediate")?,
            );
            return Ok(true);
        }
        "stop" => Command::Stop,
        "play" => Command::SetMode(SurfaceMode::Play),
        "clips" => Command::SetMode(SurfaceMode::Clips),
        "chords" => Command::SetMode(SurfaceMode::Chords),
        "harmony" => Command::SetHarmony(
            serde_json::from_str(argument).map_err(|e| format!("invalid harmony settings: {e}"))?,
        ),
        "profile" => {
            let profile = serde_json::from_str(argument)
                .map_err(|e| format!("invalid output profile: {e}"))?;
            host.configure_output(DestinationId(1), profile)
                .map_err(|e| e.to_string())?;
            println!("{:?}", host.output_routes());
            return Ok(true);
        }
        "save" => {
            storage::save_document(
                Path::new(argument),
                &host.document().map_err(|e| e.to_string())?,
            )?;
            return Ok(true);
        }
        "open" => {
            // Keep output ownership while loading; never discard pending releases.
            if host.engine.snapshot().active_voices != 0 {
                return Err("stop before opening a session".into());
            }
            let document = storage::load_document(Path::new(argument))?;
            if document
                .session
                .tracks
                .iter()
                .any(|t| t.instrument.destination != DestinationId(1))
            {
                return Err("CLI connects only destination 1".into());
            }
            let mut replacement =
                TestHost::from_document(document, Geometry::Pads200).map_err(|e| e.to_string())?;
            replacement.launch_quantum = host.launch_quantum;
            replacement
                .advance_micros(micros)
                .map_err(|e| e.to_string())?;
            replacement
                .command(Command::SetMode(SurfaceMode::Play))
                .map_err(|e| e.to_string())?;
            *host = replacement;
            return Ok(true);
        }
        _ => return Err("unknown command".into()),
    };
    host.command(command).map_err(|e| e.to_string())?;
    Ok(true)
}
