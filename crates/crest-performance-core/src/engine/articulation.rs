//! Delayed chord attacks share the musical clock and reserved voice ownership.
use super::*;

impl Engine {
    pub(super) fn next_harmony_attack(&self, target: BeatTime) -> Option<(usize, InputEvent)> {
        self.active
            .iter()
            .enumerate()
            .filter_map(|(index, voice)| voice.pending_attack.map(|event| (index, event)))
            .filter(|(_, event)| event.at <= target)
            .min_by_key(|(index, event)| (event.at, *index))
    }

    pub(super) fn sound_pending_chord_voice(
        &mut self,
        index: usize,
        event: InputEvent,
    ) -> Result<()> {
        if self.output.len() == self.limits.output_events {
            return Err(Error::Capacity("output; drain and retry"));
        }
        self.check_record_capacity(event)?;
        self.active[index].pending_attack = None;
        self.emit_harmony(self.active[index], event);
        Ok(())
    }

    /// A pending recorded tone reserves its attack and eventual release. Once
    /// sounded, the recorder's held-note reservation replaces these two slots.
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
