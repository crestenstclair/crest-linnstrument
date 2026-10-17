//! Played voicings assigned to the seven harmony pads. These are fixed MIDI
//! pitches; captured bends and pressure remain performance expression, not tuning.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};

pub const MAX_VOICING_NOTES: usize = 5;

/// A validated, sorted voicing. Unisons retain separate voices. The explicit
/// root may be absent from the played notes, for rootless voicings and bass routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ChordDefinition", into = "ChordDefinition")]
pub struct LearnedChord {
    notes: [u8; MAX_VOICING_NOTES],
    count: u8,
    root: u8,
}

impl LearnedChord {
    pub fn new(pitches: &[u8], root: u8) -> Result<Self> {
        if pitches.is_empty()
            || pitches.len() > MAX_VOICING_NOTES
            || root > 127
            || pitches.iter().any(|pitch| *pitch > 127)
        {
            return Err(Error::Invalid(
                "learned chord needs 1–5 MIDI pitches and a MIDI root",
            ));
        }
        let mut notes = [0; MAX_VOICING_NOTES];
        notes[..pitches.len()].copy_from_slice(pitches);
        notes[..pitches.len()].sort_unstable();
        Ok(Self {
            notes,
            count: pitches.len() as u8,
            root,
        })
    }
    pub fn notes(&self) -> &[u8] {
        &self.notes[..usize::from(self.count)]
    }
    pub fn root(self) -> u8 {
        self.root
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ChordDefinition {
    notes: Vec<u8>,
    root: u8,
}
impl TryFrom<ChordDefinition> for LearnedChord {
    type Error = Error;
    fn try_from(value: ChordDefinition) -> Result<Self> {
        Self::new(&value.notes, value.root)
    }
}
impl From<LearnedChord> for ChordDefinition {
    fn from(value: LearnedChord) -> Self {
        Self {
            notes: value.notes().to_vec(),
            root: value.root,
        }
    }
}
