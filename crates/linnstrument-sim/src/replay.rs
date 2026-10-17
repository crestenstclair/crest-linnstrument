use linnstrument_sim::GestureRun;
use std::path::Path;
fn main() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        return Err("usage: linnstrument-replay RUN.json WORKER FLASH.bin".into());
    }
    let run = GestureRun::load(Path::new(&args[0]))?;
    let steps = run.verify(Path::new(&args[1]), Path::new(&args[2]))?;
    println!(
        "Verified {steps} firmware steps: exact MIDI, LED frames, timestamps and peripheral counters."
    );
    Ok(())
}
