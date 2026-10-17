use super::*;

impl Engine {
    pub(super) fn generate(&mut self, slot: usize, settings: PatternSettings) -> Result<()> {
        self.ensure_editable(slot)?;
        let clip = settings.render(self.session.harmony, self.session.slots[slot].track)?;
        if clip.events.len() > self.limits.recorded_events {
            return Err(Error::Capacity("generated clip events"));
        }
        self.undo = Some((slot, self.session.slots[slot].clip.replace(clip)));
        Ok(())
    }
    pub(super) fn follow_harmony(&mut self, slot: usize, enabled: bool) -> Result<()> {
        self.ensure_editable(slot)?;
        let mut clip = self.session.slots[slot]
            .clip
            .clone()
            .ok_or(Error::NotFound)?;
        if enabled && clip.follow.is_none() {
            clip.follow = Some(HarmonyFollow {
                reference: self.session.harmony,
            });
        } else if !enabled {
            clip.follow = None;
        }
        self.undo = Some((slot, self.session.slots[slot].clip.replace(clip)));
        Ok(())
    }
    pub(super) fn freeze_harmony(&mut self, slot: usize) -> Result<()> {
        self.ensure_editable(slot)?;
        let mut clip = self.session.slots[slot]
            .clip
            .clone()
            .ok_or(Error::NotFound)?;
        let follow = clip
            .follow
            .ok_or(Error::State("clip is not following harmony"))?;
        for event in &mut clip.events {
            if let Gesture::On { key, .. } = &mut event.gesture {
                *key = follow.pitch(*key, self.session.harmony)?;
            }
        }
        clip.follow = None;
        clip.validate(self.session.slots[slot].track)?;
        self.undo = Some((slot, self.session.slots[slot].clip.replace(clip)));
        Ok(())
    }
    pub(super) fn set_harmony(&mut self, settings: HarmonySettings) -> Result<()> {
        settings.validate()?;
        if settings
            .bass_track
            .is_some_and(|id| !self.session.tracks.iter().any(|t| t.id == id))
        {
            return Err(Error::NotFound);
        }
        self.session.harmony = settings;
        Ok(())
    }
    pub(super) fn harmony_input(&mut self, event: InputEvent) -> Result<()> {
        if !self.session.tracks.iter().any(|t| t.id == event.note.track) {
            return Err(Error::NotFound);
        }
        match event.gesture {
            Gesture::On {
                key,
                velocity,
                expression,
            } => self.attack_chord(event, key, velocity, expression),
            _ => self.update_chord(event),
        }
    }
    fn attack_chord(
        &mut self,
        trigger: InputEvent,
        key: u8,
        velocity: u8,
        expression: Expression,
    ) -> Result<()> {
        let owner = Owner::Harmony(trigger.note);
        if self.active.iter().any(|v| v.owner == owner) {
            return Err(Error::DuplicateNote);
        }
        let ChordAttack {
            degree,
            voicing,
            events,
            expressive,
        } = self.prepare_chord(trigger, key, velocity, expression)?;
        self.check_harmony_capacity(&events, true)?;
        let mut captured = false;
        for (index, event) in events
            .into_iter()
            .enumerate()
            .filter_map(|(i, e)| e.map(|e| (i, e)))
        {
            let mut voice = self.allocate_voice(owner, event.note.track)?;
            voice.rendered_note = event.note;
            voice.expressive = expressive[index];
            *self.active.last_mut().expect("allocated voice") = voice;
            captured |= self.emit_harmony(voice, event);
        }
        self.session.harmony.degree = degree;
        self.last_voicing = Some(voicing);
        self.remember_harmony(trigger, captured);
        Ok(())
    }
    fn prepare_chord(
        &self,
        trigger: InputEvent,
        key: u8,
        velocity: u8,
        expression: Expression,
    ) -> Result<ChordAttack> {
        let settings = self.session.harmony;
        let step = i16::from(key) - i16::from(settings.input_anchor);
        let degree = if settings.playing == HarmonyPlaying::ChordPads {
            step.rem_euclid(7) as u8
        } else {
            settings.degree
        };
        let voicing = settings.voicing(degree, self.last_voicing.as_ref())?;
        let count = if settings.playing == HarmonyPlaying::Strumplate {
            1
        } else {
            voicing.notes().len()
        };
        let bass = settings
            .bass_track
            .filter(|_| settings.playing == HarmonyPlaying::ChordPads);
        let mut events = [None; MAX_CHORD_VOICES];
        let mut expressive = [true; MAX_CHORD_VOICES];
        for index in 0..count + usize::from(bass.is_some()) {
            let is_bass = index == count;
            let pitch = if is_bass {
                voicing
                    .root()
                    .checked_sub(12)
                    .ok_or(Error::Invalid("bass below MIDI range"))?
            } else if settings.playing == HarmonyPlaying::Strumplate {
                voicing.strumplate_pitch(step)?
            } else {
                voicing.notes()[index]
            };
            expressive[index] = count == 1
                || settings.expression == ExpressionPolicy::AllVoices
                || index == count - 1;
            let id = self
                .next_voice
                .checked_add(index as u64)
                .ok_or(Error::Capacity("voice identities"))?;
            events[index] = Some(InputEvent {
                at: trigger.at,
                note: NoteKey {
                    source: GENERATED_SOURCE,
                    track: if is_bass {
                        bass.expect("bass present")
                    } else {
                        trigger.note.track
                    },
                    note: NoteId(id),
                },
                gesture: Gesture::On {
                    key: pitch,
                    velocity,
                    expression: if expressive[index] {
                        expression
                    } else {
                        Expression::default()
                    },
                },
            });
        }
        Ok(ChordAttack {
            degree,
            voicing,
            events,
            expressive,
        })
    }
    fn update_chord(&mut self, trigger: InputEvent) -> Result<()> {
        let owner = Owner::Harmony(trigger.note);
        let mut events = [None; MAX_CHORD_VOICES];
        let mut voices = [None; MAX_CHORD_VOICES];
        let mut found = false;
        let mut count = 0;
        for voice in self.active.iter().filter(|v| v.owner == owner) {
            found = true;
            if matches!(trigger.gesture, Gesture::Expression { .. }) && !voice.expressive {
                continue;
            }
            events[count] = Some(InputEvent {
                note: voice.rendered_note,
                ..trigger
            });
            voices[count] = Some(*voice);
            count += 1;
        }
        if !found {
            return Err(Error::UnknownNote);
        }
        self.check_harmony_capacity(&events, false)?;
        let mut captured = false;
        for (voice, event) in voices.into_iter().zip(events).filter_map(|(v, e)| v.zip(e)) {
            captured |= self.emit_harmony(voice, event);
        }
        if matches!(trigger.gesture, Gesture::Off { .. }) {
            self.active.retain(|v| v.owner != owner);
        }
        self.remember_harmony(trigger, captured);
        Ok(())
    }
    /// Preflight the entire fan-out, including reserved recording releases.
    /// No partial chord or partial expression update can escape on rejection.
    fn check_harmony_capacity(
        &self,
        events: &[Option<InputEvent>; MAX_CHORD_VOICES],
        attack: bool,
    ) -> Result<()> {
        let count = events.iter().flatten().count();
        if self.output.len() + count > self.limits.output_events {
            return Err(Error::Capacity("output; drain and retry"));
        }
        if attack {
            if self.active.len() + count > self.limits.voices {
                return Err(Error::Capacity("voices; chord rejected"));
            }
            self.next_voice
                .checked_add(count as u64)
                .ok_or(Error::Capacity("voice identities"))?;
        }
        let Some(recording) = &self.recording else {
            return Ok(());
        };
        let mut held = recording.held.len();
        let mut captured = 0;
        for event in events
            .iter()
            .flatten()
            .filter(|e| e.note.track == self.session.slots[recording.slot].track)
        {
            match event.gesture {
                Gesture::On { .. } => held += 1,
                _ if !recording.held.contains(&event.note) => continue,
                Gesture::Off { .. } => held -= 1,
                _ => {}
            }
            captured += 1;
        }
        if recording.events.len() + captured + held > self.limits.recorded_events {
            return Err(Error::Capacity("recording; finish or cancel take"));
        }
        Ok(())
    }
    fn emit_harmony(&mut self, voice: ActiveVoice, event: InputEvent) -> bool {
        self.output.push_back(OutputEvent {
            at: event.at,
            destination: voice.destination,
            track: voice.track,
            voice: voice.id,
            gesture: event.gesture,
        });
        let captured = self.capture(event);
        self.remember(event);
        captured
    }
    fn remember_harmony(&mut self, input: InputEvent, captured: bool) {
        let origin = HarmonyGesture {
            algorithm_version: HARMONY_ALGORITHM_VERSION,
            input,
            settings: self.session.harmony,
        };
        if self.recent_harmony.len() == self.limits.recorded_events
            && let Some(evicted) = self.recent_harmony.pop_front()
        {
            self.history_floor = self
                .history_floor
                .max(BeatTime(evicted.input.at.0.saturating_add(1)));
        }
        self.recent_harmony.push_back(origin);
        if captured && let Some(recording) = &mut self.recording {
            // Each retained intent has at least one rendered event, so the
            // rendered-event limit also bounds this preallocated metadata.
            debug_assert!(recording.harmony_origin.len() < self.limits.recorded_events);
            let mut origin = origin;
            origin.input.at.0 -= recording.start.0;
            recording.harmony_origin.push(origin);
        }
    }
}

struct ChordAttack {
    degree: u8,
    voicing: Voicing,
    events: [Option<InputEvent>; MAX_CHORD_VOICES],
    expressive: [bool; MAX_CHORD_VOICES],
}
