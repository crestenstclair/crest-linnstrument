#[cfg(feature = "desktop")]
#[tauri::command]
fn simulator_command(
    bridge: tauri::State<'_, linnstrument_sim::Bridge>,
    command: linnstrument_sim::BridgeCommand,
) -> Result<(), String> {
    bridge.send(command)
}
#[cfg(feature = "desktop")]
#[tauri::command]
fn simulator_snapshot(
    bridge: tauri::State<'_, linnstrument_sim::Bridge>,
) -> Result<String, String> {
    bridge.snapshot()
}
#[cfg(feature = "desktop")]
fn main() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let model = std::env::var("LINNSTRUMENT_MODEL")
        .unwrap_or_else(|_| "200".into())
        .parse()
        .expect("model must be 128 or 200");
    let binary_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_owned));
    let packaged = binary_dir.filter(|p| p.join("linnstrument-worker").is_file());
    let assets = packaged.unwrap_or_else(|| root.join("target/firmware-native"));
    let worker = std::env::var_os("LINNSTRUMENT_WORKER")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| assets.join("linnstrument-worker"));
    let flash = std::env::var_os("LINNSTRUMENT_FLASH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| assets.join(format!("ideal-{model}.bin")));
    let external = std::env::var("LINNSTRUMENT_VIRTUAL_MIDI").is_ok_and(|v| v == "1");
    let bridge = linnstrument_sim::Bridge::start_with_midi(&worker, &flash, model, external)
        .expect("build firmware worker with scripts/build-native.sh first");
    tauri::Builder::default()
        .manage(bridge)
        .invoke_handler(tauri::generate_handler![
            simulator_command,
            simulator_snapshot
        ])
        .run(tauri::generate_context!())
        .expect("webview shell failed");
}
#[cfg(not(feature = "desktop"))]
fn main() {
    eprintln!(
        "Use --features desktop for the Tauri replica. Headless firmware: scripts/test-firmware.py"
    );
}
