//! Standalone host's stock User Firmware Mode layout. Both OS MIDI and the
//! simulator enter through the same decoded pad events; LEDs remain intent.
use super::*;

impl TestHost {
    pub(super) fn accept_clip_pad(&mut self, pad: PadEvent) -> Result<()> {
        if !pad.pressed || pad.column == 0 {
            return Ok(());
        }
        let index = usize::from(pad.column - 1);
        match pad.row {
            0 if index < self.engine.session().slots.len() => {
                match self.engine.snapshot().slots[index] {
                    SlotState::Playing | SlotState::Queued => {
                        self.engine.command(Command::StopSlot { slot: index })?
                    }
                    SlotState::Recorded => self.launch(index)?,
                    _ => {}
                }
            }
            7 if index < 7 => {
                let settings = HarmonySettings {
                    degree: index as u8,
                    ..self.engine.session().harmony
                };
                // Validate before taking over a clip leader. An invalid degree
                // must leave both the selected chord and its source unchanged.
                self.engine.command(Command::SetHarmony(settings))?;
                self.engine
                    .command(Command::SetHarmonySource(HarmonySource::Selected {}))?;
            }
            _ => {}
        }
        Ok(())
    }
}

pub(super) fn frame(state: &PerformanceSnapshot) -> [[u8; 8]; 26] {
    let mut frame = [[0; 8]; 26];
    for (slot, status) in state.slots.iter().take(25).enumerate() {
        frame[slot + 1][0] = if state.muted_slots[slot] {
            MUTED_SLOT_COLOR
        } else {
            slot_color(*status)
        };
    }
    for degree in 0..7 {
        // Show actual follower intent, including a running recorded leader.
        frame[usize::from(degree) + 1][7] = if degree == state.effective_harmony.degree {
            3 // green: active degree
        } else {
            5 // blue: other degree selectors
        };
    }
    frame
}
