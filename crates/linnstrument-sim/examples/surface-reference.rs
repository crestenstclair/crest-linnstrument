//! Capture read-only manual references from the same native worker as the GUI.
//! Run after scripts/build-native.sh; outputs are not interactive MIDI devices.
use crest_performance_core::{HarmonyPlaying, HarmonySettings, SurfaceMode};
use linnstrument_sim::{Bridge, BridgeCommand};
use serde_json::Value;
use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

fn wait_for(bridge: &Bridge, predicate: impl Fn(&Value) -> bool) -> Result<Value, String> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = bridge.snapshot()?;
        if !snapshot.is_empty() {
            let state: Value = serde_json::from_str(&snapshot).map_err(|e| e.to_string())?;
            if let Some(error) = state["error"].as_str() {
                return Err(error.into());
            }
            if predicate(&state) {
                return Ok(state);
            }
        }
        if Instant::now() >= deadline {
            return Err("snapshot timeout".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
}
fn capture(bridge: &Bridge, output: &Path, name: &str) -> Result<(), String> {
    let before: Value = serde_json::from_str(&bridge.snapshot()?).map_err(|e| e.to_string())?;
    let at = before["firmware"]["at_us"]
        .as_u64()
        .ok_or("missing firmware time")?;
    for _ in 0..20 {
        bridge.send(BridgeCommand::Step { micros: 10_000 })?;
    }
    let state = wait_for(bridge, |s| {
        s["firmware"]["at_us"].as_u64().unwrap_or(0) >= at + 200_000
    })?;
    std::fs::write(
        output.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&state).unwrap(),
    )
    .map_err(|e| e.to_string())
}
fn main() -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let assets = root.join("target/firmware-native");
    let output = root.join("target/manual/surfaces");
    std::fs::create_dir_all(&output).map_err(|e| e.to_string())?;
    let bridge = Bridge::start(
        &assets.join("linnstrument-worker"),
        &assets.join("ideal-200.bin"),
        200,
    )?;
    bridge.send(BridgeCommand::Pause { paused: true })?;
    wait_for(&bridge, |s| s["paused"] == true)?;
    bridge.send(BridgeCommand::Open {
        path: root
            .join("examples/performance-showcase.json")
            .display()
            .to_string(),
    })?;
    bridge.send(BridgeCommand::Surface {
        mode: SurfaceMode::Play,
    })?;
    wait_for(&bridge, |s| {
        s["host"]["performance"]["slots"][0] == "recorded"
            && s["host"]["performance"]["mode"] == "play"
    })?;
    capture(&bridge, &output, "play")?;
    bridge.send(BridgeCommand::Harmony {
        settings: HarmonySettings {
            input_octaves: true,
            input_anchor: 28,
            ..HarmonySettings::default()
        },
    })?;
    bridge.send(BridgeCommand::Surface {
        mode: SurfaceMode::Chords,
    })?;
    wait_for(&bridge, |s| s["host"]["performance"]["mode"] == "chords")?;
    capture(&bridge, &output, "chords")?;
    bridge.send(BridgeCommand::Harmony {
        settings: HarmonySettings {
            playing: HarmonyPlaying::Strumplate,
            input_octaves: true,
            input_anchor: 28,
            ..HarmonySettings::default()
        },
    })?;
    wait_for(&bridge, |s| {
        s["host"]["performance"]["harmony"]["playing"] == "strumplate"
    })?;
    capture(&bridge, &output, "strumplate")?;
    bridge.send(BridgeCommand::Surface {
        mode: SurfaceMode::Clips,
    })?;
    wait_for(&bridge, |s| s["host"]["performance"]["mode"] == "clips")?;
    capture(&bridge, &output, "clips")?;
    bridge.send(BridgeCommand::Launch { slot: 0 })?;
    wait_for(&bridge, |s| {
        s["host"]["performance"]["slots"][0] == "playing"
    })?;
    capture(&bridge, &output, "clips-playing")?;
    bridge.send(BridgeCommand::Stop {})?;
    println!("{}", output.display());
    Ok(())
}
