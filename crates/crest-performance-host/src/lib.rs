//! Standalone host orchestration. Uses only public library contracts.
mod clip_surface;
mod holds;
#[cfg(feature = "midi")]
pub mod midi_ports;
#[cfg(feature = "midi")]
pub mod midi_runner;
pub mod storage;
use crest_linnstrument::*;
use crest_performance_core::*;
use crest_performance_testkit::CapturedMidi;
pub use holds::OctaveDirection;
use holds::{ControlSource, Holds};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Serialize)]
pub struct HostSnapshot {
    pub launch_quantum: BeatTime,
    pub tempo_milli_bpm: u32,
    pub outputs: Vec<OutputRoute>,
    pub performance: PerformanceSnapshot,
    pub clips: Vec<Option<Clip>>,
    pub tracks: Vec<Track>,
    pub capture_count: u64,
    pub recent_midi: Vec<CapturedMidi>,
    pub scene_morph: Option<SceneMorph>,
    pub parameter_capture_count: u64,
    pub recent_parameters: Vec<ParameterEvent>,
}
#[derive(Debug, Clone, Serialize)]
pub struct OutputRoute {
    pub destination: DestinationId,
    pub profile: OutputProfile,
    pub capabilities: OutputCapabilities,
}
pub struct TestHost {
    pub engine: Engine,
    /// Live host launch preference, in ticks; zero launches immediately.
    /// Applies to future GUI/CLI/controller launches, never moves queued clips.
    pub launch_quantum: BeatTime,
    input: NoteInput,
    parser: MidiParser,
    outputs: BTreeMap<DestinationId, MidiOutput>,
    clock: ClockMapping,
    last_micros: u64,
    pub capture: Vec<CapturedMidi>,
    capture_count: u64,
    /// Development sink only; these are not silently converted to MIDI or DSP.
    pub parameter_capture: Vec<ParameterEvent>,
    parameter_capture_count: u64,
    pub device_commands: Vec<MidiMessage>,
    leds: LedRenderer,
    scene_row: UserRowX,
    geometry: Geometry,
    mode: SurfaceMode,
    holds: Holds,
    clip_controls: clip_surface::Controls,
}
impl TestHost {
    pub fn new(session: Session, geometry: Geometry) -> Result<Self> {
        let input_track = session
            .tracks
            .first()
            .ok_or(Error::Invalid("test host needs an input track"))?
            .id;
        let outputs = session
            .tracks
            .iter()
            .map(|track| {
                let id = track.instrument.destination;
                Ok((id, MidiOutput::new(id, OutputProfile::default())?))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        let mut host = Self {
            holds: Holds::new(session.slots.len()),
            clip_controls: clip_surface::Controls::default(),
            engine: Engine::new(
                API_VERSION,
                session,
                Limits {
                    voices: 15,
                    ..Limits::default()
                },
            )?,
            input: NoteInput::mpe(MpeZone::lower(), SourceId(1), input_track),
            launch_quantum: BeatTime(0),
            parser: MidiParser::default(),
            outputs,
            clock: ClockMapping::new(0, BeatTime(0), storage::DEFAULT_TEMPO_MILLI_BPM)?,
            last_micros: 0,
            capture: Vec::with_capacity(65536),
            capture_count: 0,
            parameter_capture: Vec::with_capacity(65536),
            parameter_capture_count: 0,
            device_commands: Vec::new(),
            leds: LedRenderer::new(geometry),
            scene_row: UserRowX::new(2, geometry)?,
            geometry,
            mode: SurfaceMode::Play,
        };
        host.advertise_capture_parameters()?;
        // Deliberate test-backend contract: 16 voice-scoped normalized controls
        // per destination. They have no implicit MIDI CC or production DSP map.
        let note_addresses: Vec<_> = host
            .outputs
            .keys()
            .flat_map(|destination| note_capture_parameters(*destination))
            .collect();
        host.engine
            .set_note_parameter_capabilities(&note_addresses)?;
        Ok(host)
    }
    /// Configure A/B endpoints for the explicit parameter capture sink. No MIDI
    /// controller mapping or production instrument capability is implied.
    pub fn set_scene_morph(&mut self, scene: Option<SceneMorph>) -> Result<()> {
        self.engine.set_scene_morph(scene)?;
        self.advertise_capture_parameters()?;
        self.configure_scene_row();
        self.paint()
    }
    fn advertise_capture_parameters(&mut self) -> Result<()> {
        let addresses: Vec<_> = self
            .engine
            .session()
            .scene_morph
            .as_ref()
            .map(|scene| scene.parameters().iter().map(|p| p.address).collect())
            .unwrap_or_default();
        self.engine.set_parameter_capabilities(&addresses)
    }
    pub fn from_document(document: storage::HostDocument, geometry: Geometry) -> Result<Self> {
        document.validate()?;
        let mut host = Self::new(document.session, geometry)?;
        host.clock = ClockMapping::new(0, BeatTime(0), document.tempo_milli_bpm)?;
        for route in document.outputs {
            host.outputs.insert(
                route.destination,
                MidiOutput::new(route.destination, route.profile)?,
            );
        }
        Ok(host)
    }
    pub fn document(&self) -> Result<storage::HostDocument> {
        Ok(storage::HostDocument {
            host_version: storage::HOST_VERSION,
            tempo_milli_bpm: self.clock.milli_bpm(),
            session: self.engine.save_session()?,
            outputs: self
                .outputs
                .iter()
                .map(|(destination, output)| storage::OutputRouteConfig {
                    destination: *destination,
                    profile: output.profile(),
                })
                .collect(),
        })
    }
    /// Control-thread routing change. Stop live notes and all clip players first.
    /// Profiles are host backend configuration, separate from the core session.
    pub fn configure_output(
        &mut self,
        destination: DestinationId,
        profile: OutputProfile,
    ) -> Result<()> {
        let snapshot = self.engine.snapshot();
        if snapshot.active_voices != 0
            || snapshot.slots.iter().any(|s| {
                matches!(
                    s,
                    SlotState::Playing
                        | SlotState::Queued
                        | SlotState::Recording
                        | SlotState::Overdubbing
                )
            })
        {
            return Err(Error::State(
                "stop transport before changing output profile",
            ));
        }
        if !self.outputs.contains_key(&destination) {
            return Err(Error::NotFound);
        }
        let output = MidiOutput::new(destination, profile)?;
        for message in output.configuration() {
            append_capture(
                &mut self.capture,
                &mut self.capture_count,
                CapturedMidi {
                    at: snapshot.at,
                    destination,
                    message,
                },
            );
        }
        self.outputs.insert(destination, output);
        Ok(())
    }
    pub fn output_routes(&self) -> Vec<OutputRoute> {
        self.outputs
            .iter()
            .map(|(destination, output)| OutputRoute {
                destination: *destination,
                profile: output.profile(),
                capabilities: output
                    .profile()
                    .capabilities()
                    .expect("validated output profile"),
            })
            .collect()
    }
    pub fn accept_bytes(&mut self, micros: u64, byte: u8, geometry: Geometry) -> Result<()> {
        self.advance_micros(micros)?;
        let at = self.engine.position();
        let Some(message) = self.parser.feed(byte) else {
            return Ok(());
        };
        if self.mode == SurfaceMode::Clips {
            if self.engine.session().scene_morph.is_some()
                && let Some(position) = self.scene_row.accept(message)?
            {
                self.engine.command(Command::SetMorphPosition(position))?;
            }
            if let Some(pad) = user_pad(message, geometry)? {
                self.accept_clip_pad(pad)?;
            }
        } else {
            for event in self.input.accept(at, message)? {
                self.engine.input(*event)?;
            }
        }
        self.capture_output()?;
        Ok(())
    }
    pub fn advance_micros(&mut self, micros: u64) -> Result<()> {
        // Reject native time reversal even when both timestamps round to one tick.
        if micros < self.last_micros {
            return Err(Error::TimeOrder);
        }
        self.advance(self.clock.at_micros(micros)?)?;
        self.last_micros = micros;
        Ok(())
    }
    /// Re-anchor the host clock at a native timestamp. Finish scheduled work
    /// under the old tempo first; beat position, held ownership and clip phase
    /// do not jump. Validation precedes any advancement or output changes.
    pub fn set_tempo(&mut self, micros: u64, milli_bpm: u32) -> Result<()> {
        let next = self.clock.with_tempo(micros, milli_bpm)?;
        self.advance_micros(micros)?;
        self.clock = next;
        Ok(())
    }
    pub fn launch(&mut self, slot: usize) -> Result<()> {
        self.command(Command::Launch {
            slot,
            quantum: self.launch_quantum,
        })
    }
    fn advance(&mut self, at: BeatTime) -> Result<()> {
        self.engine.advance(at)?;
        self.capture_output()
    }
    pub fn command(&mut self, command: Command) -> Result<()> {
        let replaced_slot =
            match command {
                Command::Launch { slot, .. }
                | Command::StopSlot { slot }
                | Command::BeginOverdub { slot }
                | Command::SetClipTranspose { slot, .. } => Some(slot),
                Command::FinishRecord => self.engine.snapshot().slots.iter().position(|state| {
                    matches!(state, SlotState::Recording | SlotState::Overdubbing)
                }),
                _ => None,
            };
        match command {
            Command::SetFill(held) => self.set_fill(ControlSource::Host, held)?,
            Command::SetClipMuted { slot, muted } => {
                self.set_muted(ControlSource::Host, slot, muted)?;
            }
            _ => self.engine.command(command)?,
        }
        // A pair of opposing holds has a net offset of zero. Retire those
        // owners explicitly on player replacement and safety transitions too.
        if matches!(command, Command::Stop | Command::SetMode(_)) {
            self.holds.reset_octaves(None);
        } else if let Some(slot) = replaced_slot {
            self.holds.reset_octaves(Some(slot));
        }
        self.holds.reconcile(&self.engine.snapshot());
        if matches!(command, Command::Stop) {
            self.input.reset();
            self.scene_row.reset();
            self.clip_controls = clip_surface::Controls::default();
        }
        if let Command::SetMode(mode) = command {
            self.mode = mode;
            self.clip_controls = clip_surface::Controls::default();
            self.input.reset();
            self.parser = MidiParser::default();
            self.device_commands
                .extend(user_mode(mode == SurfaceMode::Clips));
            self.leds.invalidate();
            self.configure_scene_row();
        }
        self.capture_output()?;
        self.paint()
    }
    fn configure_scene_row(&mut self) {
        self.scene_row.reset();
        if self.mode == SurfaceMode::Clips {
            self.device_commands.extend(
                self.scene_row
                    .configuration(self.engine.session().scene_morph.is_some()),
            );
        }
    }
    pub fn paint(&mut self) -> Result<()> {
        if self.mode != SurfaceMode::Clips {
            return Ok(());
        }
        let snapshot = self.engine.snapshot();
        let mut frame = clip_surface::frame(&snapshot);
        if self.engine.session().scene_morph.is_some() {
            let span = u32::from(self.geometry.columns() - 2);
            let selected = snapshot
                .morph_position
                .map(|position| 1 + (u32::from(position) * span + 32_767) / 65_535);
            for column in 1..self.geometry.columns() {
                frame[usize::from(column)][2] = if selected == Some(u32::from(column)) {
                    3
                } else {
                    5
                };
            }
        }
        // Firmware owns the global-settings control-column indicator.
        self.device_commands.extend(self.leds.diff(&frame, 16)?);
        Ok(())
    }
    fn capture_output(&mut self) -> Result<()> {
        for event in self.engine.drain_parameter_output() {
            append_capture(
                &mut self.parameter_capture,
                &mut self.parameter_capture_count,
                event,
            );
        }
        for event in self.engine.drain_output() {
            let batch = self
                .outputs
                .get_mut(&event.destination)
                .ok_or(Error::NotFound)?
                .accept(event)?;
            for message in batch.messages() {
                append_capture(
                    &mut self.capture,
                    &mut self.capture_count,
                    CapturedMidi {
                        at: event.at,
                        destination: event.destination,
                        message: *message,
                    },
                );
            }
        }
        Ok(())
    }
    pub fn snapshot(&self) -> HostSnapshot {
        HostSnapshot {
            launch_quantum: self.launch_quantum,
            tempo_milli_bpm: self.clock.milli_bpm(),
            outputs: self.output_routes(),
            performance: self.engine.snapshot(),
            tracks: self.engine.session().tracks.clone(),
            clips: self
                .engine
                .session()
                .slots
                .iter()
                .map(|s| s.clip.clone())
                .collect(),
            capture_count: self.capture_count,
            scene_morph: self.engine.session().scene_morph.clone(),
            parameter_capture_count: self.parameter_capture_count,
            recent_parameters: self
                .parameter_capture
                .iter()
                .rev()
                .take(24)
                .rev()
                .copied()
                .collect(),
            recent_midi: self.capture.iter().rev().take(24).rev().cloned().collect(),
        }
    }
    /// Read newly captured output without treating the diagnostic ring as an
    /// unbounded transport queue. A slow consumer must stop on expired history.
    pub fn output_since(&self, after: u64) -> Result<&[CapturedMidi]> {
        let first = self.capture_count - self.capture.len() as u64;
        if after < first || after > self.capture_count {
            return Err(Error::Capacity("MIDI output consumer fell behind"));
        }
        Ok(&self.capture[(after - first) as usize..])
    }
    pub fn output_count(&self) -> u64 {
        self.capture_count
    }
    /// Send configuration before admitting notes to connected destinations.
    pub fn output_configuration(&self) -> Vec<CapturedMidi> {
        self.outputs
            .iter()
            .flat_map(|(destination, output)| {
                output
                    .configuration()
                    .into_iter()
                    .map(|message| CapturedMidi {
                        at: self.engine.snapshot().at,
                        destination: *destination,
                        message,
                    })
            })
            .collect()
    }
    pub fn disconnect(&mut self) -> Result<()> {
        // Abandon unsent musical work, then release the voices actually
        // allocated at each sink. Unallocated rejected attacks must not stop
        // cleanup of unrelated sounding voices with UnknownNote errors.
        self.engine.drain_output().for_each(drop);
        self.engine.drain_parameter_output().for_each(drop);
        self.engine.command(Command::Stop)?;
        self.holds.reset_octaves(None);
        self.holds.reconcile(&self.engine.snapshot());
        self.engine.drain_output().for_each(drop);
        self.input.reset();
        self.scene_row.reset();
        self.clip_controls = clip_surface::Controls::default();
        self.parser = MidiParser::default();
        let at = self.engine.snapshot().at;
        for (destination, output) in &mut self.outputs {
            for message in output.release_all() {
                append_capture(
                    &mut self.capture,
                    &mut self.capture_count,
                    CapturedMidi {
                        at,
                        destination: *destination,
                        message,
                    },
                );
            }
        }
        Ok(())
    }
}

fn note_capture_parameters(destination: DestinationId) -> impl Iterator<Item = ParameterAddress> {
    (0..16).map(move |parameter| ParameterAddress {
        destination,
        parameter,
    })
}

fn append_capture<T>(capture: &mut Vec<T>, count: &mut u64, message: T) {
    *count += 1;
    if capture.len() == 65536 {
        capture.drain(..32768);
    }
    capture.push(message);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn output_profiles_negotiate_persist_and_cannot_change_held_ownership() {
        let mut host = TestHost::new(Session::default(), Geometry::Pads200).unwrap();
        let profile = OutputProfile::Monophonic {
            channel: 0,
            bend_semitones: 48,
        };
        host.configure_output(DestinationId(1), profile).unwrap();
        for byte in [0x91, 60, 90, 0x92, 64, 90] {
            host.accept_bytes(0, byte, Geometry::Pads200).unwrap();
        }
        let before = host.output_count();
        assert!(
            host.configure_output(DestinationId(1), OutputProfile::default())
                .is_err()
        );
        assert_eq!(host.output_count(), before);
        for byte in [0x82, 64, 0] {
            host.accept_bytes(1, byte, Geometry::Pads200).unwrap();
        }
        assert_eq!(host.capture.last().unwrap().message.bytes(), [0x90, 60, 90]);
        host.disconnect().unwrap();
        let document = host.document().unwrap();
        let path = std::env::temp_dir().join(format!("crest-profile-{}.json", std::process::id()));
        storage::save_document(&path, &document).unwrap();
        let loaded = storage::load_document(&path).unwrap();
        let mut reopened = TestHost::from_document(loaded, Geometry::Pads200).unwrap();
        assert_eq!(reopened.output_routes()[0].profile, profile);
        for byte in [0x91, 67, 90] {
            reopened.accept_bytes(0, byte, Geometry::Pads200).unwrap();
        }
        assert_eq!(
            reopened.capture.last().unwrap().message.bytes(),
            [0x90, 67, 90]
        );
        let mut invalid = document.clone();
        invalid.outputs.push(invalid.outputs[0].clone());
        assert!(storage::save_document(&path, &invalid).is_err());
        assert_eq!(storage::load_document(&path).unwrap(), document);
        // Legacy plain core sessions remain readable and explicitly get defaults.
        storage::save_session(&path, &document.session).unwrap();
        assert_eq!(
            storage::load_document(&path).unwrap().outputs[0].profile,
            OutputProfile::default()
        );
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn disconnect_releases_allocated_voices_after_an_output_rejection() {
        let mut host = TestHost::new(Session::default(), Geometry::Pads200).unwrap();
        host.outputs.insert(
            DestinationId(1),
            MidiOutput::new(
                DestinationId(1),
                OutputProfile::Mpe {
                    main: 0,
                    first: 1,
                    last: 1,
                    bend_semitones: 48,
                },
            )
            .unwrap(),
        );
        for byte in [0x91, 60, 90] {
            host.accept_bytes(0, byte, Geometry::Pads200).unwrap();
        }
        for byte in [0x92, 64] {
            host.accept_bytes(1, byte, Geometry::Pads200).unwrap();
        }
        assert!(host.accept_bytes(1, 90, Geometry::Pads200).is_err());
        host.disconnect().unwrap();
        assert_eq!(host.engine.snapshot().active_voices, 0);
        assert!(
            host.capture
                .iter()
                .any(|m| m.message.bytes() == [0x81, 60, 0])
        );
        assert!(
            !host
                .capture
                .iter()
                .any(|m| m.message.bytes() == [0x81, 64, 0])
        );
        for byte in [0x91, 67, 90] {
            host.accept_bytes(2, byte, Geometry::Pads200).unwrap();
        }
        assert_eq!(host.engine.snapshot().active_voices, 1);
    }
}
