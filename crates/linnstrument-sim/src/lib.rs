//! Typed contact bridge; the native scheduler, never browser frames, advances firmware.
#[cfg(test)]
mod condition_tests;
mod contact_envelope;
#[cfg(test)]
mod controller_hold_tests;
#[cfg(test)]
mod harmony_source_tests;
#[cfg(test)]
mod harmony_tests;
#[cfg(test)]
mod humanized_tests;
#[cfg(test)]
mod quantize_tests;
#[cfg(test)]
mod tempo_tests;
pub use contact_envelope::ContactProfile;
use contact_envelope::PressureRamp;
mod scenario;
mod trace;
#[cfg(feature = "midi")]
mod virtual_midi;
mod worker;
use crest_linnstrument::*;
use crest_performance_core::{
    BeatTime, Command, HarmonySettings, HarmonySource, NoteCondition, NoteEdit, NoteKey,
    PatternSettings, Session, SurfaceMode,
};
use crest_performance_host::{HostSnapshot, TestHost};
pub use scenario::{ContactAction, ContactScenario, ContactStep};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender},
    },
    time::{Duration, Instant},
};
pub use trace::GestureRun;
pub use worker::*;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Contact {
    pub id: u32,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub latched: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum BridgeCommand {
    Contact(Contact),
    ContactProfile {
        profile: ContactProfile,
    },
    Release {
        id: u32,
    },
    CancelContact {
        id: u32,
    },
    ReleaseMomentary {},
    ReleaseAll {},
    FootSwitch {
        number: u8,
        pressed: bool,
    },
    Pause {
        paused: bool,
    },
    Step {
        micros: u32,
    },
    Reset {},
    BeginRecord {
        slot: usize,
    },
    BeginOverdub {
        slot: usize,
    },
    CaptureRecent {
        slot: usize,
        ticks: u64,
    },
    Duplicate {
        from: usize,
        to: usize,
    },
    EditNote {
        slot: usize,
        note: NoteKey,
        start: u64,
        duration: u64,
        key: u8,
    },
    NoteCondition {
        slot: usize,
        note: NoteKey,
        condition: NoteCondition,
    },
    QuantizeClip {
        slot: usize,
        ticks: u64,
    },
    Fill {
        held: bool,
    },
    ClipMuted {
        slot: usize,
        muted: bool,
    },
    Undo {},
    FinishRecord {},
    CancelRecord {},
    Launch {
        slot: usize,
    },
    LaunchQuantum {
        ticks: u64,
    },
    Tempo {
        milli_bpm: u32,
    },
    Stop {},
    Mode {
        clips: bool,
    },
    Surface {
        mode: SurfaceMode,
    },
    Harmony {
        settings: HarmonySettings,
    },
    HarmonySource {
        source: HarmonySource,
    },
    LearnChord {
        slot: usize,
        degree: u8,
        root: Option<u8>,
    },
    ClearLearnedChord {
        degree: u8,
    },
    Generate {
        slot: usize,
        settings: PatternSettings,
    },
    FollowHarmony {
        slot: usize,
        enabled: bool,
    },
    FreezeHarmony {
        slot: usize,
    },
    StopSlot {
        slot: usize,
    },
    OutputProfile {
        destination: crest_performance_core::DestinationId,
        profile: OutputProfile,
    },
    Save {
        path: String,
    },
    Open {
        path: String,
    },
    ExportRun {
        path: String,
    },
    VerifyRun {
        path: String,
    },
    PlayExample {},
    RunScenario {
        path: String,
    },
}
#[derive(Serialize)]
pub struct DisplaySnapshot {
    pub scenario_steps_remaining: usize,
    pub run_steps: usize,
    pub run_full: bool,
    pub recent_firmware_midi: Vec<(u64, u8)>,
    pub verified_steps: Option<usize>,
    pub external_host: bool,
    pub connected: bool,
    pub error: Option<String>,
    pub paused: bool,
    pub contacts: Vec<Contact>,
    pub contact_profile: ContactProfile,
    pub scenario_running: bool,
    pub firmware: FirmwareSnapshot,
    pub host: HostSnapshot,
}
pub struct Bridge {
    sender: SyncSender<TimedCommand>,
    snapshot: Arc<Mutex<String>>,
    overflow: Arc<AtomicBool>,
}
impl Bridge {
    pub fn start(executable: &Path, flash: &Path, model: u16) -> Result<Self, String> {
        Self::start_with_midi(executable, flash, model, false)
    }
    pub fn start_with_midi(
        executable: &Path,
        flash: &Path,
        model: u16,
        external: bool,
    ) -> Result<Self, String> {
        #[cfg(feature = "midi")]
        let midi = if external {
            Some(virtual_midi::VirtualMidi::new()?)
        } else {
            None
        };
        #[cfg(not(feature = "midi"))]
        if external {
            return Err("rebuild with midi feature for OS virtual ports".into());
        }
        let (worker, firmware) = Worker::start(executable, flash, model)?;
        let run = GestureRun::new(&firmware, flash)?;
        let geometry = match model {
            128 => Geometry::Pads128,
            200 => Geometry::Pads200,
            _ => return Err("invalid model".into()),
        };
        let host = TestHost::new(Session::default(), geometry).map_err(|e| e.to_string())?;
        let (sender, receiver) = mpsc::sync_channel(256);
        let snapshot = Arc::new(Mutex::new(String::new()));
        let shared = Arc::clone(&snapshot);
        let overflow = Arc::new(AtomicBool::new(false));
        let scheduler_overflow = Arc::clone(&overflow);
        let executable = executable.to_owned();
        let flash = flash.to_owned();
        std::thread::spawn(move || {
            let runtime = Runtime {
                worker,
                firmware,
                host,
                contacts: Vec::new(),
                contact_profile: ContactProfile::default(),
                ramps: Vec::with_capacity(16),
                paused: false,
                geometry,
                connected: true,
                run,
                recent_midi: std::collections::VecDeque::with_capacity(256),
                verified_steps: None,
                scenario: None,
                #[cfg(feature = "midi")]
                midi,
            };
            run_scheduler(
                runtime,
                receiver,
                shared,
                scheduler_overflow,
                &executable,
                &flash,
                model,
            );
        });
        Ok(Self {
            sender,
            snapshot,
            overflow,
        })
    }
    pub fn send(&self, command: BridgeCommand) -> Result<(), String> {
        self.sender
            .try_send(TimedCommand {
                received: Instant::now(),
                command,
            })
            .map_err(|e| {
                if matches!(e, mpsc::TrySendError::Full(_)) {
                    // This out-of-band latch cannot itself be lost in the full
                    // queue. The scheduler silences before accepting more work.
                    self.overflow.store(true, Ordering::Release);
                }
                format!("simulator command queue: {e}")
            })
    }
    pub fn snapshot(&self) -> Result<String, String> {
        self.snapshot
            .lock()
            .map(|s| s.clone())
            .map_err(|_| "snapshot lock poisoned".into())
    }
}
struct TimedCommand {
    received: Instant,
    command: BridgeCommand,
}
fn run_scheduler(
    mut runtime: Runtime,
    receiver: Receiver<TimedCommand>,
    shared: Arc<Mutex<String>>,
    overflow: Arc<AtomicBool>,
    executable: &Path,
    flash: &Path,
    model: u16,
) {
    let mut last = Instant::now();
    let mut last_display = last;
    let mut error = None;
    loop {
        let message = receiver.recv_timeout(Duration::from_millis(5));
        if matches!(message, Err(mpsc::RecvTimeoutError::Disconnected)) {
            runtime.fail_closed();
            break;
        }
        if overflow.swap(false, Ordering::AcqRel) {
            // Drop stale gestures, including the message already received.
            receiver.try_iter().take(256).for_each(drop);
            runtime.fail_closed();
            error = Some(
                "simulator command queue overflow: output silenced; reset device to reconnect"
                    .into(),
            );
            runtime.publish(&shared, error.clone(), false);
            last = Instant::now();
            continue;
        }
        let received = message
            .as_ref()
            .map_or_else(|_| Instant::now(), |m| m.received);
        let elapsed = received
            .saturating_duration_since(last)
            .as_micros()
            .min(50_000) as u32;
        last = received;
        if runtime.connected
            && !runtime.paused
            && elapsed > 0
            && let Err(problem) = runtime.advance(elapsed)
        {
            runtime.fail_closed();
            error = Some(problem.clone());
            runtime.publish(&shared, Some(problem), false);
        }
        let changed = message.is_ok();
        if let Ok(message) = message {
            let result = match message.command {
                BridgeCommand::Reset {} => runtime.reset(executable, flash, model),
                BridgeCommand::VerifyRun { path } => runtime.verify_run(&path, executable, flash),
                command => runtime.command(command),
            };
            error = result.err();
            if !runtime.connected {
                runtime.fail_closed();
            }
        }
        if changed || last_display.elapsed() >= Duration::from_millis(33) {
            runtime.publish(&shared, error.clone(), runtime.connected);
            last_display = Instant::now();
        }
    }
}
struct Runtime {
    scenario: Option<scenario::Playback>,
    run: GestureRun,
    recent_midi: std::collections::VecDeque<(u64, u8)>,
    verified_steps: Option<usize>,
    #[cfg(feature = "midi")]
    midi: Option<virtual_midi::VirtualMidi>,
    worker: Worker,
    firmware: FirmwareSnapshot,
    host: TestHost,
    contacts: Vec<Contact>,
    contact_profile: ContactProfile,
    ramps: Vec<PressureRamp>,
    paused: bool,
    geometry: Geometry,
    connected: bool,
}
impl Runtime {
    fn fail_closed(&mut self) {
        self.connected = false;
        self.paused = true;
        self.scenario = None;
        self.contacts.clear();
        self.ramps.clear();
        let _ = self.host.disconnect();
        #[cfg(feature = "midi")]
        if let Some(midi) = &mut self.midi {
            midi.reset();
        }
    }
    fn external(&self) -> bool {
        #[cfg(feature = "midi")]
        {
            self.midi.is_some()
        }
        #[cfg(not(feature = "midi"))]
        {
            false
        }
    }
    fn reset(&mut self, executable: &Path, flash: &Path, model: u16) -> Result<(), String> {
        let document = self.host.document().map_err(|e| e.to_string())?;
        #[cfg(feature = "midi")]
        if let Some(midi) = &mut self.midi {
            midi.reset();
        }
        self.host.disconnect().map_err(|e| e.to_string())?;
        let (worker, firmware) =
            Worker::start(executable, flash, model).inspect_err(|_| self.connected = false)?;
        self.run = GestureRun::new(&firmware, flash)?;
        self.recent_midi.clear();
        self.verified_steps = None;
        self.scenario = None;
        self.worker = worker;
        self.firmware = firmware;
        let mut host =
            TestHost::from_document(document, self.geometry).map_err(|e| e.to_string())?;
        host.launch_quantum = self.host.launch_quantum;
        self.host = host;
        self.contacts.clear();
        self.ramps.clear();
        self.paused = false;
        self.connected = true;
        Ok(())
    }
    fn request(&mut self, line: &str) -> Result<(), String> {
        let result = self.request_inner(line);
        if result.is_err() {
            self.connected = false;
        }
        result
    }
    fn request_inner(&mut self, line: &str) -> Result<(), String> {
        let snapshot = self
            .worker
            .request(line)
            .inspect_err(|_| self.connected = false)?;
        self.run.record(line, &snapshot);
        for event in &snapshot.midi {
            if self.recent_midi.len() == 256 {
                self.recent_midi.pop_front();
            }
            self.recent_midi.push_back(*event);
        }
        #[cfg(feature = "midi")]
        if let Some(midi) = &mut self.midi {
            midi.emit(&snapshot.midi)?;
        }
        if !self.external() {
            for (at, byte) in &snapshot.midi {
                self.host
                    .accept_bytes(*at, *byte, self.geometry)
                    .map_err(|e| e.to_string())?;
            }
        }
        self.host
            .advance_micros(snapshot.at_us)
            .map_err(|e| e.to_string())?;
        self.firmware = snapshot;
        Ok(())
    }
    fn advance(&mut self, micros: u32) -> Result<(), String> {
        #[cfg(feature = "midi")]
        for _ in 0..256 {
            let Some(midi) = &self.midi else {
                break;
            };
            let Some(command) = midi.feedback()? else {
                break;
            };
            self.request(&command)?;
        }
        self.flush_device_commands()?;
        self.advance_contacts(micros)?;
        self.host.paint().map_err(|e| e.to_string())?;
        self.flush_device_commands()
    }
    fn flush_device_commands(&mut self) -> Result<(), String> {
        if !self.host.device_commands.is_empty() {
            let messages = std::mem::take(&mut self.host.device_commands);
            let bytes: Vec<String> = messages
                .iter()
                .flat_map(MidiMessage::as_bytes)
                .map(u8::to_string)
                .collect();
            self.request(&format!("midi {}", bytes.join(" ")))?;
        }
        Ok(())
    }
    fn start_scenario(&mut self, scenario: ContactScenario) -> Result<(), String> {
        if self.scenario.is_some() || !self.contacts.is_empty() {
            return Err("release contacts and stop any current script first".into());
        }
        scenario.validate(self.firmware.model, self.geometry)?;
        self.scenario = Some(scenario::Playback {
            started_us: self.firmware.at_us,
            steps: scenario.steps.into(),
        });
        Ok(())
    }
    fn next_scenario_time(&mut self) -> Result<Option<u64>, String> {
        let Some(playback) = &self.scenario else {
            return Ok(None);
        };
        let Some(step) = playback.steps.front() else {
            // The script owns its release tails until the native envelopes end.
            if self.ramps.is_empty() {
                self.scenario = None;
            }
            return Ok(None);
        };
        playback
            .started_us
            .checked_add(step.at_us)
            .map(Some)
            .ok_or("scenario time overflow".into())
    }
    fn apply_scenario_step(&mut self) -> Result<(), String> {
        if self
            .next_scenario_time()?
            .is_none_or(|at| at > self.firmware.at_us)
        {
            return Ok(());
        }
        let action = self
            .scenario
            .as_mut()
            .unwrap()
            .steps
            .pop_front()
            .unwrap()
            .action;
        match action {
            ContactAction::Contact(contact) => self.set_contact(contact),
            ContactAction::Release { id } => self.release(id),
            ContactAction::ReleaseAll {} => {
                self.request("release_all")?;
                self.contacts.clear();
                self.ramps.clear();
                Ok(())
            }
        }
    }
    fn advance_contacts(&mut self, micros: u32) -> Result<(), String> {
        let target = self
            .firmware
            .at_us
            .checked_add(u64::from(micros))
            .ok_or("scenario clock overflow")?;
        loop {
            let scenario = self.next_scenario_time()?;
            let envelope = self.ramps.iter().map(|r| r.next).min();
            let Some(next) = scenario.into_iter().chain(envelope).min() else {
                break;
            };
            if next > target {
                break;
            }
            if next > self.firmware.at_us {
                self.request(&format!("advance {}", next - self.firmware.at_us))?;
            }
            self.advance_envelopes()?;
            self.apply_scenario_step()?;
        }
        if target > self.firmware.at_us {
            self.request(&format!("advance {}", target - self.firmware.at_us))?;
        }
        Ok(())
    }
    fn command(&mut self, command: BridgeCommand) -> Result<(), String> {
        if !self.connected && !matches!(command, BridgeCommand::ExportRun { .. }) {
            return Err(
                "firmware disconnected; reset device before sending performance input".into(),
            );
        }
        match command {
            BridgeCommand::Contact(contact) => {
                if self.scenario.is_some() {
                    return Err("stop the script before manual contacts".into());
                }
                self.set_contact(contact)
            }
            BridgeCommand::ContactProfile { profile } => self.set_contact_profile(profile),
            BridgeCommand::Release { id } => {
                if self.scenario.is_some() {
                    return Err("stop the script before manual contacts".into());
                }
                self.release(id)
            }
            BridgeCommand::CancelContact { id } => {
                if self.scenario.is_some() {
                    return Err("stop the script before manual contacts".into());
                }
                self.release_immediately(id)
            }
            BridgeCommand::ReleaseAll {} => {
                self.scenario = None;
                self.request("release_all")?;
                self.contacts.clear();
                self.ramps.clear();
                Ok(())
            }
            BridgeCommand::ReleaseMomentary {} => self.release_momentary(),
            BridgeCommand::FootSwitch { number, pressed } => {
                if !(1..=2).contains(&number) {
                    return Err("foot switch must be 1 or 2".into());
                }
                self.request(&format!("switch {number} {}", u8::from(pressed)))
            }
            BridgeCommand::Pause { paused } => {
                self.paused = paused;
                Ok(())
            }
            BridgeCommand::Step { micros } => {
                if !self.paused || micros > 1_000_000 {
                    return Err("step requires pause and <= 1 second".into());
                }
                self.advance(micros)
            }
            BridgeCommand::BeginRecord { slot } => self.host_command(Command::BeginRecord { slot }),
            BridgeCommand::BeginOverdub { slot } => {
                self.host_command(Command::BeginOverdub { slot })
            }
            BridgeCommand::CaptureRecent { slot, ticks } => {
                let cutoff = self
                    .host
                    .engine
                    .position()
                    .checked_add(BeatTime(1))
                    .map_err(|error| error.to_string())?;
                self.host_command(Command::CaptureRecentAt {
                    slot,
                    length: BeatTime(ticks),
                    cutoff,
                })
            }
            BridgeCommand::Duplicate { from, to } => {
                self.host_command(Command::Duplicate { from, to })
            }
            BridgeCommand::EditNote {
                slot,
                note,
                start,
                duration,
                key,
            } => self.host_command(Command::EditNote {
                slot,
                note,
                edit: NoteEdit {
                    start: BeatTime(start),
                    duration: BeatTime(duration),
                    key,
                },
            }),
            BridgeCommand::Undo {} => self.host_command(Command::Undo),
            BridgeCommand::NoteCondition {
                slot,
                note,
                condition,
            } => self.host_command(Command::SetNoteCondition {
                slot,
                note,
                condition,
            }),
            BridgeCommand::Fill { held } => self.host_command(Command::SetFill(held)),
            BridgeCommand::QuantizeClip { slot, ticks } => {
                self.host_command(Command::QuantizeClip {
                    slot,
                    quantum: BeatTime(ticks),
                })
            }
            BridgeCommand::ClipMuted { slot, muted } => {
                self.host_command(Command::SetClipMuted { slot, muted })
            }
            BridgeCommand::FinishRecord {} => self.host_command(Command::FinishRecord),
            BridgeCommand::CancelRecord {} => self.host_command(Command::CancelRecord),
            BridgeCommand::Launch { slot } => {
                if self.external() {
                    return Err("OS MIDI mode: launch in the connected external host".into());
                }
                self.host.launch(slot).map_err(|e| e.to_string())
            }
            BridgeCommand::LaunchQuantum { ticks } => {
                if self.external() {
                    return Err("OS MIDI mode: configure the connected external host".into());
                }
                self.host.launch_quantum = BeatTime(ticks);
                Ok(())
            }
            BridgeCommand::Stop {} => self.host_command(Command::Stop),
            BridgeCommand::Tempo { milli_bpm } => {
                if self.external() {
                    return Err("OS MIDI mode: set tempo in the connected external host".into());
                }
                self.host
                    .set_tempo(self.firmware.at_us, milli_bpm)
                    .map_err(|e| e.to_string())
            }
            BridgeCommand::Mode { clips } => self.set_surface(if clips {
                SurfaceMode::Clips
            } else {
                SurfaceMode::Play
            }),
            BridgeCommand::Surface { mode } => self.set_surface(mode),
            BridgeCommand::Harmony { settings } => self.host_command(Command::SetHarmony(settings)),
            BridgeCommand::HarmonySource { source } => {
                self.host_command(Command::SetHarmonySource(source))
            }
            BridgeCommand::LearnChord { slot, degree, root } => {
                let track = self
                    .host
                    .engine
                    .session()
                    .slots
                    .get(slot)
                    .ok_or("unknown clip slot")?
                    .track;
                self.host_command(Command::LearnChord {
                    source: crest_performance_core::SourceId(1),
                    track,
                    degree,
                    root,
                })
            }
            BridgeCommand::ClearLearnedChord { degree } => {
                self.host_command(Command::ClearLearnedChord { degree })
            }
            BridgeCommand::Generate { slot, settings } => {
                self.host_command(Command::Generate { slot, settings })
            }
            BridgeCommand::FollowHarmony { slot, enabled } => {
                self.host_command(Command::FollowHarmony { slot, enabled })
            }
            BridgeCommand::FreezeHarmony { slot } => {
                self.host_command(Command::FreezeHarmony { slot })
            }
            BridgeCommand::StopSlot { slot } => self.host_command(Command::StopSlot { slot }),
            BridgeCommand::OutputProfile {
                destination,
                profile,
            } => {
                if self.external() {
                    return Err("OS MIDI mode: configure the external host".into());
                }
                self.host
                    .configure_output(destination, profile)
                    .map_err(|e| e.to_string())
            }
            BridgeCommand::Save { path } => self.save(&path),
            BridgeCommand::Open { path } => self.open(&path),
            BridgeCommand::ExportRun { path } => self.run.save(Path::new(&path)),
            BridgeCommand::VerifyRun { .. } => {
                Err("replay verification is handled by worker owner".into())
            }
            BridgeCommand::PlayExample {} => {
                self.start_scenario(ContactScenario::example(self.firmware.model)?)
            }
            BridgeCommand::RunScenario { path } => {
                self.start_scenario(ContactScenario::load(Path::new(&path))?)
            }
            BridgeCommand::Reset {} => Err("reset is handled by worker owner".into()),
        }
    }
    fn set_surface(&mut self, mode: SurfaceMode) -> Result<(), String> {
        if self.external() {
            return Err("OS MIDI mode: use the connected external host".into());
        }
        if self.host.engine.snapshot().slots.iter().any(|s| {
            matches!(
                s,
                crest_performance_core::SlotState::Recording
                    | crest_performance_core::SlotState::Overdubbing
            )
        }) {
            return Err("finish or cancel recording before changing mode".into());
        }
        self.request("release_all")?;
        self.contacts.clear();
        self.ramps.clear();
        self.scenario = None;
        self.advance(100_000)?;
        self.host_command(Command::SetMode(mode))?;
        // Finish firmware mode parsing before accepting the next contact.
        self.advance(100_000)
    }
    fn host_command(&mut self, command: Command) -> Result<(), String> {
        if self.external() {
            return Err("OS MIDI mode: use the connected external host".into());
        }
        self.host.command(command).map_err(|e| e.to_string())
    }
    fn verify_run(&mut self, path: &str, executable: &Path, flash: &Path) -> Result<(), String> {
        if !self.paused
            || !self.contacts.is_empty()
            || self.host.engine.snapshot().active_voices != 0
        {
            return Err(
                "release all contacts, stop transport, and pause before replay verification".into(),
            );
        }
        self.verified_steps = Some(GestureRun::load(Path::new(path))?.verify(executable, flash)?);
        Ok(())
    }
    fn release_momentary(&mut self) -> Result<(), String> {
        if self.scenario.is_some() {
            return Ok(());
        }
        let ids: Vec<_> = self
            .contacts
            .iter()
            .filter(|c| !c.latched)
            .map(|c| c.id)
            .collect();
        for id in ids {
            self.release_immediately(id)?;
        }
        Ok(())
    }
    fn save(&self, path: &str) -> Result<(), String> {
        crest_performance_host::storage::save_document(
            Path::new(path),
            &self.host.document().map_err(|e| e.to_string())?,
        )
    }
    fn open(&mut self, path: &str) -> Result<(), String> {
        if self.external() {
            return Err("OS MIDI mode: open sessions in the external host".into());
        }
        let document = crest_performance_host::storage::load_document(Path::new(path))?;
        let mut host =
            TestHost::from_document(document, self.geometry).map_err(|e| e.to_string())?;
        host.launch_quantum = self.host.launch_quantum;
        self.scenario = None;
        self.request("release_all")?;
        self.advance(100_000)?;
        self.host.disconnect().map_err(|e| e.to_string())?;
        host.advance_micros(self.firmware.at_us)
            .map_err(|e| e.to_string())?;
        self.contacts.clear();
        self.ramps.clear();
        self.host = host;
        // Reopening restores ordinary playing through the firmware parser.
        self.host_command(Command::SetMode(SurfaceMode::Play))?;
        self.advance(100_000)
    }
    fn publish(&self, shared: &Mutex<String>, error: Option<String>, connected: bool) {
        let snapshot = DisplaySnapshot {
            scenario_steps_remaining: self.scenario.as_ref().map_or(0, |s| s.steps.len()),
            run_steps: self.run.steps(),
            run_full: self.run.full(),
            recent_firmware_midi: self.recent_midi.iter().copied().collect(),
            verified_steps: self.verified_steps,
            external_host: self.external(),
            connected,
            error,
            paused: self.paused,
            contacts: self.contacts.clone(),
            contact_profile: self.contact_profile,
            scenario_running: self.scenario.is_some(),
            firmware: self.firmware.clone(),
            host: self.host.snapshot(),
        };
        if let Ok(json) = serde_json::to_string(&snapshot)
            && let Ok(mut value) = shared.lock()
        {
            *value = json;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bridge_rejects_unknown_fields_and_accepts_typed_contacts() {
        let command: BridgeCommand = serde_json::from_str(
            r#"{"type":"contact","id":1,"x":4.0,"y":2.0,"z":0.5,"latched":true}"#,
        )
        .unwrap();
        assert!(matches!(
            command,
            BridgeCommand::Contact(Contact { id: 1, .. })
        ));
        assert!(
            serde_json::from_str::<BridgeCommand>(r#"{"type":"release_all","unexpected":1}"#)
                .is_err()
        );
    }
    #[test]
    #[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
    fn firmware_phrase_records_reopens_and_launches_from_user_pad() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (worker, firmware) = Worker::start(
            &root.join("target/firmware-native/linnstrument-worker"),
            &root.join("target/firmware-native/ideal-200.bin"),
            200,
        )
        .unwrap();
        let mut r = Runtime {
            scenario: None,
            run: GestureRun::new(
                &firmware,
                &root.join("target/firmware-native/ideal-200.bin"),
            )
            .unwrap(),
            recent_midi: std::collections::VecDeque::new(),
            verified_steps: None,
            #[cfg(feature = "midi")]
            midi: None,
            worker,
            firmware,
            host: TestHost::new(Session::default(), Geometry::Pads200).unwrap(),
            contacts: vec![],
            contact_profile: ContactProfile::default(),
            ramps: Vec::with_capacity(16),
            paused: true,
            geometry: Geometry::Pads200,
            connected: true,
        };
        r.advance(1_000_000).unwrap();
        r.host_command(Command::BeginRecord { slot: 0 }).unwrap();
        for (id, x, y) in [(1, 4.0, 2.0), (2, 9.0, 3.0), (3, 14.0, 4.0)] {
            r.set_contact(Contact {
                id,
                x,
                y,
                z: 0.5,
                latched: false,
            })
            .unwrap();
        }
        r.advance(300_000).unwrap();
        r.set_contact(Contact {
            id: 1,
            x: 4.25,
            y: 2.0,
            z: 0.5,
            latched: false,
        })
        .unwrap();
        r.set_contact(Contact {
            id: 2,
            x: 9.0,
            y: 3.3,
            z: 0.5,
            latched: false,
        })
        .unwrap();
        r.set_contact(Contact {
            id: 3,
            x: 14.0,
            y: 4.0,
            z: 0.7,
            latched: false,
        })
        .unwrap();
        r.advance(200_000).unwrap();
        r.command(BridgeCommand::ReleaseAll {}).unwrap();
        r.advance(200_000).unwrap();
        r.host_command(Command::FinishRecord).unwrap();
        let saved = r.host.engine.save_session().unwrap();
        crest_performance_host::storage::save_session(
            &root.join("target/firmware-native/example-session.json"),
            &saved,
        )
        .unwrap();
        let clip = saved.slots[0].clip.as_ref().unwrap();
        let starts: Vec<_> = clip
            .events
            .iter()
            .filter(|e| matches!(e.gesture, crest_performance_core::Gesture::On { .. }))
            .collect();
        assert_eq!(starts.len(), 3);
        assert!(clip.events.iter().any(|e|matches!(e.gesture,crest_performance_core::Gesture::Expression{expression} if expression.pitch>100_000)));
        for attack in &starts {
            let crest_performance_core::Gesture::On { key, .. } = attack.gesture else {
                unreachable!()
            };
            let expressions: Vec<_> = clip
                .events
                .iter()
                .filter(|e| e.note == attack.note)
                .filter_map(|e| match e.gesture {
                    crest_performance_core::Gesture::On { expression, .. }
                    | crest_performance_core::Gesture::Expression { expression } => {
                        Some(expression)
                    }
                    _ => None,
                })
                .collect();
            match key {
                43 => assert!(expressions.iter().any(|e| e.pitch > 100_000)),
                53 => assert!(expressions.iter().any(|e| e.timbre > 45_000)),
                63 => assert!(expressions.iter().any(|e| e.pressure > 40_000)),
                _ => panic!("unexpected firmware pitch {key}"),
            }
            if key != 43 {
                assert!(expressions.iter().all(|e| e.pitch == 0));
            }
        }
        let bytes = serde_json::to_vec(&saved).unwrap();
        let reopened: Session = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(saved, reopened);
        r.host = TestHost::new(reopened, Geometry::Pads200).unwrap();
        r.host.advance_micros(r.firmware.at_us).unwrap();
        r.command(BridgeCommand::Mode { clips: true }).unwrap();
        r.advance(300_000).unwrap();
        r.set_contact(Contact {
            id: 9,
            x: 1.0,
            y: 0.0,
            z: 0.5,
            latched: false,
        })
        .unwrap();
        r.advance(200_000).unwrap();
        assert_eq!(
            r.host.engine.snapshot().slots[0],
            crest_performance_core::SlotState::Playing
        );
        r.command(BridgeCommand::ReleaseAll {}).unwrap();
        r.advance(200_000).unwrap();
        let green = r.firmware.leds[8];
        assert!(
            green[1] > 150 && green[0] < 20 && green[2] < 20,
            "{green:?}"
        );
        assert!(
            r.host
                .capture
                .iter()
                .any(|m| m.message.bytes()[0] & 0xf0 == 0x90)
        );
        r.command(BridgeCommand::Mode { clips: false }).unwrap();
        r.set_contact(Contact {
            id: 10,
            x: 4.0,
            y: 2.0,
            z: 0.5,
            latched: false,
        })
        .unwrap();
        r.advance(300_000).unwrap();
        r.advance(300_000).unwrap();
        let mut playing = [None; 16];
        let mut overlapping_pitch = false;
        for captured in &r.host.capture {
            let [status, key, velocity] = captured.message.bytes();
            let channel = usize::from(status & 15);
            match status & 0xf0 {
                0x90 if velocity > 0 => {
                    assert!(
                        playing[channel].is_none(),
                        "output member reused while sounding"
                    );
                    playing[channel] = Some(key);
                }
                0x80 => {
                    assert_eq!(playing[channel], Some(key));
                    playing[channel] = None;
                }
                _ => {}
            }
            overlapping_pitch |= playing.iter().filter(|key| **key == Some(43)).count() > 1;
        }
        assert!(
            overlapping_pitch,
            "live note and recorded same pitch need independent channels"
        );
        r.command(BridgeCommand::ReleaseAll {}).unwrap();
        r.advance(200_000).unwrap();
        r.host.disconnect().unwrap();
        assert_eq!(r.host.engine.snapshot().active_voices, 0);
        let trace_path = root.join("target/firmware-native/manual-run.json");
        r.run.save(&trace_path).unwrap();
        assert_eq!(
            GestureRun::load(&trace_path)
                .unwrap()
                .verify(
                    &root.join("target/firmware-native/linnstrument-worker"),
                    &root.join("target/firmware-native/ideal-200.bin")
                )
                .unwrap(),
            r.run.steps()
        );
        r.reset(
            &root.join("target/firmware-native/linnstrument-worker"),
            &root.join("target/firmware-native/ideal-200.bin"),
            200,
        )
        .unwrap();
        assert!(
            r.host.engine.session().slots[0].clip.is_some(),
            "device reset must preserve recorded clips"
        );
        let mut corrupted: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&trace_path).unwrap()).unwrap();
        corrupted["steps"][0]["command"] = serde_json::json!("save /tmp/should-never-write");
        let invalid_path = root.join("target/firmware-native/invalid-run.json");
        std::fs::write(&invalid_path, serde_json::to_vec(&corrupted).unwrap()).unwrap();
        assert!(GestureRun::load(&invalid_path).is_err());
        r.worker.kill_for_test();
        assert!(r.advance(1000).is_err());
        assert!(!r.connected);
        assert!(
            r.command(BridgeCommand::Contact(Contact {
                id: 1,
                x: 4.0,
                y: 2.0,
                z: 0.5,
                latched: false
            }))
            .is_err()
        );
        r.reset(
            &root.join("target/firmware-native/linnstrument-worker"),
            &root.join("target/firmware-native/ideal-200.bin"),
            200,
        )
        .unwrap();
        assert!(r.connected);
        r.command(BridgeCommand::BeginRecord { slot: 1 }).unwrap();
        r.command(BridgeCommand::PlayExample {}).unwrap();
        r.advance(350_000).unwrap();
        assert_eq!(r.contacts.len(), 3);
        assert_eq!(r.host.engine.snapshot().active_voices, 3);
        r.advance(650_000).unwrap();
        assert!(r.contacts.is_empty() && r.scenario.is_none());
        r.command(BridgeCommand::FinishRecord {}).unwrap();
        let scripted = r.host.engine.session().slots[1].clip.as_ref().unwrap();
        assert_eq!(
            scripted
                .events
                .iter()
                .filter(|e| matches!(e.gesture, crest_performance_core::Gesture::On { .. }))
                .count(),
            3
        );
        assert!(scripted.events.iter().any(|e|matches!(e.gesture,crest_performance_core::Gesture::Expression {expression} if expression.pitch>200_000)));
        r.command(BridgeCommand::PlayExample {}).unwrap();
        r.advance(100_000).unwrap();
        r.command(BridgeCommand::ReleaseAll {}).unwrap();
        r.advance(900_000).unwrap();
        assert!(r.contacts.is_empty() && r.scenario.is_none());
        assert_eq!(r.host.engine.snapshot().active_voices, 0);
    }
    #[test]
    #[ignore = "requires scripts/build-native.sh; explicitly run by firmware CI"]
    fn worker_death_is_reported_as_disconnect() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (mut worker, _) = Worker::start(
            &root.join("target/firmware-native/linnstrument-worker"),
            &root.join("target/firmware-native/ideal-200.bin"),
            200,
        )
        .unwrap();
        worker.kill_for_test();
        assert!(worker.request("advance 1000").is_err());
    }
    #[cfg(all(feature = "midi", unix))]
    #[test]
    #[ignore = "requires native worker and OS MIDI service"]
    fn firmware_sensor_to_os_midi_to_shared_host_and_led_return() {
        use crest_performance_host::midi_ports::*;
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let (worker, firmware) = Worker::start(
            &root.join("target/firmware-native/linnstrument-worker"),
            &root.join("target/firmware-native/ideal-200.bin"),
            200,
        )
        .unwrap();
        let mut runtime = Runtime {
            scenario: None,
            run: GestureRun::new(
                &firmware,
                &root.join("target/firmware-native/ideal-200.bin"),
            )
            .unwrap(),
            recent_midi: std::collections::VecDeque::new(),
            verified_steps: None,
            midi: Some(virtual_midi::VirtualMidi::new().unwrap()),
            worker,
            firmware,
            host: TestHost::new(Session::default(), Geometry::Pads200).unwrap(),
            contacts: vec![],
            contact_profile: ContactProfile::default(),
            ramps: Vec::with_capacity(16),
            paused: true,
            geometry: Geometry::Pads200,
            connected: true,
        };
        let input_id = input_ports()
            .unwrap()
            .into_iter()
            .find(|p| p.name.contains("LinnStrument Simulator performance"))
            .unwrap()
            .id;
        let output_id = output_ports()
            .unwrap()
            .into_iter()
            .find(|p| p.name.contains("LinnStrument Simulator feedback"))
            .unwrap()
            .id;
        let input = InputPort::connect(&input_id, Instant::now()).unwrap();
        let mut feedback = OutputPort::connect(&output_id).unwrap();
        let mut host = TestHost::new(Session::default(), Geometry::Pads200).unwrap();
        runtime.advance(1_000_000).unwrap();
        runtime
            .set_contact(Contact {
                id: 1,
                x: 4.0,
                y: 2.0,
                z: 0.5,
                latched: false,
            })
            .unwrap();
        runtime.advance(300_000).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while host.engine.snapshot().active_voices == 0 {
            while let Some(packet) = input.poll().unwrap() {
                for byte in packet.bytes() {
                    host.accept_bytes(packet.received_us, *byte, Geometry::Pads200)
                        .unwrap();
                }
            }
            assert!(
                Instant::now() < deadline,
                "firmware note did not reach OS input"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(
            host.capture
                .iter()
                .any(|m| m.message.bytes()[0] & 0xf0 == 0x90 && m.message.bytes()[1] == 43)
        );
        assert!(
            runtime.host.capture.is_empty(),
            "external mode must not run a second musical host"
        );
        for message in user_mode(true) {
            feedback.send(message.as_bytes()).unwrap();
        }
        for bytes in [[0xb0, 20, 5], [0xb0, 21, 2], [0xb0, 22, 1]] {
            feedback.send(&bytes).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            runtime.advance(100_000).unwrap();
            if runtime.firmware.leds[5 * 8 + 2][0] > 200 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "OS feedback did not reach firmware LED SPI"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        host.disconnect().unwrap();
        assert_eq!(host.engine.snapshot().active_voices, 0);
    }
}
