//! Per-note attack settings. These bind to a fresh output voice, never to an
//! input MIDI channel or destination-wide state.
use crate::*;
use serde::{Deserialize, Serialize};

pub const MAX_NOTE_PARAMETERS: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NoteParameterLock {
    pub note: NoteKey,
    pub parameter: u32,
    pub value: u16,
}

impl Clip {
    /// Sorted, bounded attack settings for this identity. Absence means that
    /// the backend initializes the voice from its instrument defaults.
    pub fn note_parameters(&self, note: NoteKey) -> &[NoteParameterLock] {
        let first = self
            .parameter_locks
            .partition_point(|entry| entry.note < note);
        let end = self
            .parameter_locks
            .partition_point(|entry| entry.note <= note);
        &self.parameter_locks[first..end]
    }

    pub(crate) fn validate_parameter_locks(&self) -> Result<()> {
        let mut previous = None;
        let mut count = 0;
        for entry in &self.parameter_locks {
            let key = (entry.note, entry.parameter);
            if previous.is_some_and(|old| old >= key) {
                return Err(Error::Invalid("unordered or duplicate note parameter lock"));
            }
            if previous.is_none_or(|(note, _)| note != entry.note) {
                self.require_unique_attack(entry.note)?;
                count = 0;
            }
            count += 1;
            if count > MAX_NOTE_PARAMETERS {
                return Err(Error::Capacity("note parameters"));
            }
            previous = Some(key);
        }
        Ok(())
    }

    /// Control-thread edit. `None` removes one lock; normalized endpoints are
    /// exact. The engine command preserves the complete prior clip for Undo.
    pub fn with_note_parameter(
        &self,
        note: NoteKey,
        parameter: u32,
        value: Option<u16>,
    ) -> Result<Self> {
        self.validate(note.track)?;
        self.require_unique_attack(note)?;
        let mut clip = self.clone();
        clip.parameter_locks
            .retain(|entry| (entry.note, entry.parameter) != (note, parameter));
        if let Some(value) = value {
            clip.parameter_locks.push(NoteParameterLock {
                note,
                parameter,
                value,
            });
            clip.parameter_locks
                .sort_by_key(|entry| (entry.note, entry.parameter));
        }
        clip.validate_parameter_locks()?;
        Ok(clip)
    }
}
