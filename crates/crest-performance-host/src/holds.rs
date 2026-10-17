//! Merge independent host and device holds before changing engine state. A
//! release from one surface must never cancel a hold on the other surface.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OctaveDirection {
    Down,
    Up,
}
impl OctaveDirection {
    fn index(self) -> usize {
        match self {
            Self::Down => 0,
            Self::Up => 1,
        }
    }
}
fn octave_semitones(holds: [Hold; 2]) -> i8 {
    12 * (i8::from(holds[1].active()) - i8::from(holds[0].active()))
}

#[derive(Clone, Copy)]
pub(super) enum ControlSource {
    Host,
    Pad,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Hold {
    host: bool,
    pad: bool,
}
impl Hold {
    fn with(mut self, source: ControlSource, held: bool) -> Self {
        match source {
            ControlSource::Host => self.host = held,
            ControlSource::Pad => self.pad = held,
        }
        self
    }
    fn active(self) -> bool {
        self.host || self.pad
    }
}
pub(super) struct Holds {
    fill: Hold,
    mute: Vec<Hold>,
    octaves: Vec<[Hold; 2]>,
}
impl Holds {
    pub(super) fn new(slots: usize) -> Self {
        Self {
            fill: Hold::default(),
            mute: vec![Hold::default(); slots],
            octaves: vec![[Hold::default(); 2]; slots],
        }
    }
    pub(super) fn reset_octaves(&mut self, slot: Option<usize>) {
        match slot {
            Some(slot) => self.octaves[slot] = [Hold::default(); 2],
            None => self.octaves.fill([Hold::default(); 2]),
        }
    }
    /// Stop, mode change and player replacement clear engine holds. Retire
    /// corresponding input ownership so a late release cannot affect a new hold.
    pub(super) fn reconcile(&mut self, state: &PerformanceSnapshot) {
        if !state.fill {
            self.fill = Hold::default();
        }
        for ((holds, semitones), status) in self
            .octaves
            .iter_mut()
            .zip(&state.clip_transpositions)
            .zip(&state.slots)
        {
            if !matches!(
                status,
                SlotState::Playing | SlotState::Queued | SlotState::Overdubbing
            ) || octave_semitones(*holds) != *semitones
            {
                *holds = [Hold::default(); 2];
            }
        }
        for (hold, muted) in self.mute.iter_mut().zip(&state.muted_slots) {
            if !muted {
                *hold = Hold::default();
            }
        }
    }
}
impl TestHost {
    /// Momentary UI owner, separate from each firmware pad. Both directions
    /// combine to zero; releasing either restores the remaining held direction.
    pub fn hold_clip_octave(
        &mut self,
        slot: usize,
        direction: OctaveDirection,
        held: bool,
    ) -> Result<()> {
        self.set_octave(ControlSource::Host, slot, direction, held)?;
        self.paint()
    }
    pub(super) fn set_octave(
        &mut self,
        source: ControlSource,
        slot: usize,
        direction: OctaveDirection,
        held: bool,
    ) -> Result<()> {
        self.holds.reconcile(&self.engine.snapshot());
        let mut next = *self.holds.octaves.get(slot).ok_or(Error::NotFound)?;
        let index = direction.index();
        next[index] = next[index].with(source, held);
        if let Err(error) = self.engine.command(Command::SetClipTranspose {
            slot,
            semitones: octave_semitones(next),
        }) {
            // Releasing an opposing hold can expose an out-of-range shift.
            // Never retain a physically released owner: restore the original
            // phrase and retire this slot's holds, while reporting the failure.
            if !held {
                self.engine
                    .command(Command::SetClipTranspose { slot, semitones: 0 })?;
                self.holds.reset_octaves(Some(slot));
            }
            return Err(error);
        }
        self.holds.octaves[slot] = next;
        Ok(())
    }
    pub(super) fn set_fill(&mut self, source: ControlSource, held: bool) -> Result<()> {
        let next = self.holds.fill.with(source, held);
        self.engine.command(Command::SetFill(next.active()))?;
        self.holds.fill = next;
        Ok(())
    }
    pub(super) fn set_muted(
        &mut self,
        source: ControlSource,
        slot: usize,
        held: bool,
    ) -> Result<()> {
        let hold = self.holds.mute.get(slot).ok_or(Error::NotFound)?;
        let next = hold.with(source, held);
        // Commit input ownership only after output-capacity validation and the
        // selected player's releases succeed. Rejected presses remain retryable.
        self.engine.command(Command::SetClipMuted {
            slot,
            muted: next.active(),
        })?;
        self.holds.mute[slot] = next;
        Ok(())
    }
}
