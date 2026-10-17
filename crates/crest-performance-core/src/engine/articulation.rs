//! Delayed chord attacks share the musical clock and reserved voice ownership.
use super::*;

/// One reserved chord tone cycles between a pending attack and a sounding
/// gate. Every repeated attack receives a new identity; trigger ownership stays.
#[derive(Debug, Clone, Copy)]
pub(super) struct RepeatingTone {
    period: BeatTime,
    gate: BeatTime,
    attack: Gesture,
    next_attack: BeatTime,
    release: Option<BeatTime>,
}
impl RepeatingTone {
    pub(super) fn new(
        articulation: Articulation,
        count: usize,
        event: InputEvent,
        trigger: BeatTime,
    ) -> Result<Option<Self>> {
        let Articulation::Arpeggio { interval, gate, .. } = articulation else {
            return Ok(None);
        };
        let period = BeatTime(interval.0 * count as u64);
        Ok(Some(Self {
            period,
            gate,
            attack: event.gesture,
            next_attack: event.at.checked_add(period)?,
            release: if event.at == trigger {
                Some(event.at.checked_add(gate)?)
            } else {
                None
            },
        }))
    }
}

impl Engine {
    pub(super) fn next_harmony_event(&self, target: BeatTime) -> Option<(usize, InputEvent)> {
        self.active
            .iter()
            .enumerate()
            .filter_map(|(index, voice)| {
                let event = voice.pending_attack.or_else(|| {
                    voice.arpeggio?.release.map(|at| InputEvent {
                        at,
                        note: voice.rendered_note,
                        gesture: Gesture::Off { velocity: 0 },
                    })
                })?;
                (event.at <= target).then_some((index, event))
            })
            .min_by_key(|(index, event)| (event.at, release_order(event.gesture), *index))
    }

    pub(super) fn perform_harmony_event(&mut self, index: usize, event: InputEvent) -> Result<()> {
        if self.output.len() == self.limits.output_events {
            return Err(Error::Capacity("output; drain and retry"));
        }
        if matches!(event.gesture, Gesture::Off { .. }) {
            return self.release_arpeggio_gate(index, event);
        }
        self.sound_pending_chord_voice(index, event)
    }

    fn sound_pending_chord_voice(&mut self, index: usize, mut event: InputEvent) -> Result<()> {
        let mut voice = self.active[index];
        let mut next_identity = self.next_voice;
        if let Some(repeating) = &mut voice.arpeggio {
            // Complete every fallible check before changing ownership or emitting.
            // A gate release never depends on allocating another note identity.
            next_identity = self
                .next_voice
                .checked_add(1)
                .ok_or(Error::Capacity("voice identities"))?;
            voice.id = VoiceId(self.next_voice);
            event.note.note = NoteId(self.next_voice);
            voice.rendered_note = event.note;
            repeating.next_attack = event.at.checked_add(repeating.period)?;
            repeating.release = Some(event.at.checked_add(repeating.gate)?);
        }
        self.check_record_capacity(event)?;
        voice.pending_attack = None;
        self.next_voice = next_identity;
        self.active[index] = voice;
        self.emit_harmony(voice, event);
        Ok(())
    }

    fn release_arpeggio_gate(&mut self, index: usize, event: InputEvent) -> Result<()> {
        self.check_record_capacity(event)?;
        let voice = self.active[index];
        self.emit_harmony(voice, event);
        let repeating = self.active[index]
            .arpeggio
            .as_mut()
            .expect("scheduled arpeggio release");
        repeating.release = None;
        self.active[index].pending_attack = Some(InputEvent {
            at: repeating.next_attack,
            note: voice.rendered_note,
            gesture: repeating.attack,
        });
        Ok(())
    }

    /// A finite strum reserves pending attacks and releases. Repeating tones
    /// reserve each release when its attack is accepted; future cycles may reject
    /// on capacity, but cannot consume the reservation of a sounding release.
    pub(super) fn pending_record_cost(&self, except: Option<NoteKey>) -> usize {
        let Some(recording) = &self.recording else {
            return 0;
        };
        let track = self.session.slots[recording.slot].track;
        2 * self
            .active
            .iter()
            .filter(|voice| {
                voice.pending_attack.is_some()
                    && voice.arpeggio.is_none()
                    && voice.track == track
                    && Some(voice.rendered_note) != except
            })
            .count()
    }

    pub(super) fn recording_chord(&self, owner: Owner) -> bool {
        self.recording.as_ref().is_some_and(|recording| {
            self.active.iter().any(|voice| {
                voice.owner == owner
                    && voice.track == self.session.slots[recording.slot].track
                    && (voice.pending_attack.is_some()
                        || recording.held.contains(&voice.rendered_note))
            })
        })
    }

    pub(super) fn check_harmony_origin_capacity(
        &self,
        owner: Owner,
        gesture: Gesture,
        captured: bool,
    ) -> Result<()> {
        let Some(recording) = self.recording.as_ref().filter(|_| captured) else {
            return Ok(());
        };
        let held = self.recorded_chord_owners();
        let reserved = match gesture {
            Gesture::On { .. } => held + 1,
            Gesture::Off { .. } if self.recording_chord(owner) => held - 1,
            _ => held,
        };
        if recording.harmony_origin.len() + 1 + reserved > self.limits.recorded_events {
            return Err(Error::Capacity(
                "recorded harmony gestures; finish or cancel take",
            ));
        }
        Ok(())
    }

    fn recorded_chord_owners(&self) -> usize {
        self.active
            .iter()
            .enumerate()
            .filter(|(index, voice)| self.first_recorded_chord_owner(*index, voice.owner))
            .count()
    }

    fn first_recorded_chord_owner(&self, index: usize, owner: Owner) -> bool {
        matches!(owner, Owner::Harmony(_))
            && !self.active[..index]
                .iter()
                .any(|voice| voice.owner == owner)
            && self.recording_chord(owner)
    }

    /// Expression before an attack becomes that note's initial state. It never
    /// emits expression for a voice the destination has not heard yet.
    pub(super) fn update_pending_chord(&mut self, owner: Owner, gesture: Gesture) {
        let Gesture::Expression { expression } = gesture else {
            return;
        };
        for voice in self
            .active
            .iter_mut()
            .filter(|voice| voice.owner == owner && voice.expressive)
        {
            if let Some(repeating) = &mut voice.arpeggio
                && let Gesture::On {
                    expression: initial,
                    ..
                } = &mut repeating.attack
            {
                *initial = expression;
            }
            if let Some(InputEvent {
                gesture:
                    Gesture::On {
                        expression: initial,
                        ..
                    },
                ..
            }) = &mut voice.pending_attack
            {
                *initial = expression;
            }
        }
    }
}
