use super::*;

impl Engine {
    /// Control-thread capability negotiation for voice-scoped attack settings.
    /// This is distinct from destination-wide morph support. Values are resolved
    /// by the host, not mapped to MIDI. Capabilities never persist in sessions.
    pub fn set_note_parameter_capabilities(
        &mut self,
        addresses: &[ParameterAddress],
    ) -> Result<()> {
        if !self.parameter_output.is_empty() {
            return Err(Error::State(
                "drain parameter output before changing capabilities",
            ));
        }
        self.validate_parameter_addresses(addresses)?;
        self.note_parameter_capabilities = addresses.to_vec();
        Ok(())
    }

    fn validate_parameter_addresses(&self, addresses: &[ParameterAddress]) -> Result<()> {
        let mut previous = None;
        for address in addresses {
            if previous.is_some_and(|value| value >= *address) {
                return Err(Error::Invalid(
                    "parameter capabilities must have unique sorted addresses",
                ));
            }
            if !self
                .session
                .tracks
                .iter()
                .any(|track| track.instrument.destination == address.destination)
            {
                return Err(Error::NotFound);
            }
            previous = Some(*address);
        }
        Ok(())
    }

    pub(super) fn check_note_parameter_output(&self, owner: Owner, track: TrackId) -> Result<()> {
        let Owner::Clip(slot, note) = owner else {
            return Ok(());
        };
        let locks = self.session.slots[slot]
            .clip
            .as_ref()
            .ok_or(Error::NotFound)?
            .note_parameters(note);
        if locks.is_empty() {
            return Ok(());
        }
        let destination = self
            .session
            .tracks
            .iter()
            .find(|t| t.id == track)
            .ok_or(Error::NotFound)?
            .instrument
            .destination;
        if locks.iter().any(|entry| {
            self.note_parameter_capabilities
                .binary_search(&ParameterAddress {
                    destination,
                    parameter: entry.parameter,
                })
                .is_err()
        }) {
            return Err(Error::State(
                "destination does not support a note parameter",
            ));
        }
        if locks.len() > self.limits.output_events - self.parameter_output.len() {
            return Err(Error::Capacity("parameter output"));
        }
        Ok(())
    }

    /// Called only after capability/capacity preflight and voice allocation.
    /// Publication cannot fail halfway through the attack's parameter batch.
    pub(super) fn emit_note_parameters(&mut self, owner: Owner, voice: ActiveVoice, at: BeatTime) {
        let Owner::Clip(slot, note) = owner else {
            return;
        };
        let clip = self.session.slots[slot]
            .clip
            .as_ref()
            .expect("validated player");
        self.parameter_output
            .extend(
                clip.note_parameters(note)
                    .iter()
                    .map(|entry| ParameterEvent {
                        at,
                        voice: Some(voice.id),
                        address: ParameterAddress {
                            destination: voice.destination,
                            parameter: entry.parameter,
                        },
                        value: entry.value,
                    }),
            );
    }
    /// Control-thread configuration. Stored endpoints remain usable even when a
    /// destination is currently unsupported. No parameter is emitted implicitly.
    pub fn set_scene_morph(&mut self, scene: Option<SceneMorph>) -> Result<()> {
        if !self.parameter_output.is_empty() {
            return Err(Error::State(
                "drain parameter output before replacing scene",
            ));
        }
        if let Some(scene) = &scene {
            self.session.validate_scene(scene)?;
        }
        self.session.scene_morph = scene;
        self.morph_position = None;
        Ok(())
    }
    /// Host capability negotiation, independent of persistence. An empty set
    /// disables parameter output; reopening never assumes backend support.
    pub fn set_parameter_capabilities(&mut self, addresses: &[ParameterAddress]) -> Result<()> {
        if !self.parameter_output.is_empty() {
            return Err(Error::State(
                "drain parameter output before changing capabilities",
            ));
        }
        if addresses.len() > MAX_MORPH_PARAMETERS {
            return Err(Error::Capacity("parameter capabilities"));
        }
        self.validate_parameter_addresses(addresses)?;
        self.parameter_capabilities = addresses.to_vec();
        Ok(())
    }
    /// Separate bounded continuous-control stream. Drain alongside note output;
    /// merge by timestamp, parameters before notes at equal times. It never
    /// consumes the note queue's reserved release capacity.
    pub fn drain_parameter_output(&mut self) -> impl Iterator<Item = ParameterEvent> + '_ {
        self.parameter_output.drain(..)
    }
    pub(super) fn morph_scene(&mut self, position: u16) -> Result<()> {
        let scene = self
            .session
            .scene_morph
            .as_ref()
            .ok_or(Error::State("configure a scene morph first"))?;
        if scene.parameters().iter().any(|p| {
            self.parameter_capabilities
                .binary_search(&p.address)
                .is_err()
        }) {
            return Err(Error::State(
                "destination does not support a scene parameter",
            ));
        }
        if scene.parameters().len() > self.limits.output_events - self.parameter_output.len() {
            return Err(Error::Capacity("parameter output"));
        }
        self.parameter_output
            .extend(scene.render(self.at, position));
        self.morph_position = Some(position);
        Ok(())
    }
}
