//! Merge independent host and device holds before changing engine state. A
//! release from one surface must never cancel a hold on the other surface.
use super::*;

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
}
impl Holds {
    pub(super) fn new(slots: usize) -> Self {
        Self {
            fill: Hold::default(),
            mute: vec![Hold::default(); slots],
        }
    }
    /// Stop, mode change and player replacement clear engine holds. Retire
    /// corresponding input ownership so a late release cannot affect a new hold.
    pub(super) fn reconcile(&mut self, state: &PerformanceSnapshot) {
        if !state.fill {
            self.fill = Hold::default();
        }
        for (hold, muted) in self.mute.iter_mut().zip(&state.muted_slots) {
            if !muted {
                *hold = Hold::default();
            }
        }
    }
}
impl TestHost {
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
