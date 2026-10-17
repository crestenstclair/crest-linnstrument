//! Optional attack rules. Raw notes and expression remain lossless; a rule is
//! sampled once at attack, never again for that note's expression or release.
use crate::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum NoteCondition {
    Always {},
    /// Zero-based loop offset within a nonzero period. Each launch restarts at
    /// iteration zero; `period: 4, offset: 3` plays on loops 4, 8, 12, …
    Every {
        period: u16,
        offset: u16,
    },
    Fill {},
    NotFill {},
}
impl Default for NoteCondition {
    fn default() -> Self {
        Self::Always {}
    }
}
impl NoteCondition {
    pub fn validate(self) -> Result<()> {
        if let Self::Every { period, offset } = self
            && (period == 0 || offset >= period)
        {
            return Err(Error::Invalid("condition period or offset"));
        }
        Ok(())
    }
    /// Hosts may preview a validated rule without advancing an engine.
    pub fn accepts(self, iteration: u64, fill: bool) -> Result<bool> {
        self.validate()?;
        Ok(self.accepts_validated(iteration, fill))
    }
    pub(crate) fn accepts_validated(self, iteration: u64, fill: bool) -> bool {
        match self {
            Self::Always {} => true,
            Self::Every { period, offset } => iteration % u64::from(period) == u64::from(offset),
            Self::Fill {} => fill,
            Self::NotFill {} => !fill,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConditionalNote {
    pub note: NoteKey,
    pub condition: NoteCondition,
}

impl Clip {
    pub fn note_condition(&self, note: NoteKey) -> NoteCondition {
        self.conditions
            .binary_search_by_key(&note, |entry| entry.note)
            .map_or(NoteCondition::Always {}, |index| {
                self.conditions[index].condition
            })
    }
    pub(crate) fn validate_conditions(&self) -> Result<()> {
        if self.conditions.is_empty() {
            return Ok(());
        }
        let mut attacks = std::collections::BTreeMap::new();
        for event in &self.events {
            if matches!(event.gesture, Gesture::On { .. }) {
                *attacks.entry(event.note).or_insert(0_usize) += 1;
            }
        }
        let mut previous = None;
        for entry in &self.conditions {
            entry.condition.validate()?;
            if entry.condition == (NoteCondition::Always {})
                || previous.is_some_and(|note| note >= entry.note)
            {
                return Err(Error::Invalid(
                    "unordered, duplicate or redundant note condition",
                ));
            }
            if attacks.get(&entry.note) != Some(&1) {
                return Err(Error::Invalid("condition requires one unique note attack"));
            }
            previous = Some(entry.note);
        }
        Ok(())
    }
    pub(crate) fn require_unique_attack(&self, note: NoteKey) -> Result<()> {
        let count = self
            .events
            .iter()
            .filter(|event| event.note == note && matches!(event.gesture, Gesture::On { .. }))
            .count();
        if count != 1 {
            return Err(Error::Invalid("note metadata requires one unique attack"));
        }
        Ok(())
    }
    /// Control-thread edit. Removing a rule restores the unchanged original
    /// performance. Engine commands retain the entire old clip for Undo.
    pub fn with_note_condition(&self, note: NoteKey, condition: NoteCondition) -> Result<Self> {
        self.validate(note.track)?;
        condition.validate()?;
        self.require_unique_attack(note)?;
        let mut clip = self.clone();
        clip.conditions.retain(|entry| entry.note != note);
        if condition != (NoteCondition::Always {}) {
            clip.conditions.push(ConditionalNote { note, condition });
            clip.conditions.sort_by_key(|entry| entry.note);
        }
        Ok(clip)
    }
}
