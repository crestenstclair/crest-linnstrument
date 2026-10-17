//! Standalone host's stock User Firmware Mode layout. Both OS MIDI and the
//! simulator enter through the same decoded pad events; LEDs remain intent.
use super::*;

impl TestHost {
    pub(super) fn accept_clip_pad(&mut self, pad: PadEvent) -> Result<()> {
        if pad.column == 0 {
            return Ok(());
        }
        let index = usize::from(pad.column - 1);
        match pad.row {
            0 if pad.pressed && index < self.engine.session().slots.len() => {
                match self.engine.snapshot().slots[index] {
                    SlotState::Playing | SlotState::Queued => {
                        self.command(Command::StopSlot { slot: index })?
                    }
                    SlotState::Recorded => self.launch(index)?,
                    _ => {}
                }
            }
            1 if index < self.engine.session().slots.len() => {
                let state = self.engine.snapshot().slots[index];
                if !pad.pressed || matches!(state, SlotState::Playing | SlotState::Queued) {
                    self.set_muted(ControlSource::Pad, index, pad.pressed)?;
                }
            }
            6 if index == 0 => self.set_fill(ControlSource::Pad, pad.pressed)?,
            7 if pad.pressed && index < 7 => {
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
        if matches!(status, SlotState::Playing | SlotState::Queued) {
            frame[slot + 1][1] = if state.muted_slots[slot] {
                MUTED_SLOT_COLOR
            } else {
                5 // blue: available hold-to-mute
            };
        }
    }
    for degree in 0..7 {
        // Show actual follower intent, including a running recorded leader.
        frame[usize::from(degree) + 1][7] = if degree == state.effective_harmony.degree {
            3 // green: active degree
        } else {
            5 // blue: other degree selectors
        };
    }
    frame[1][6] = if state.fill { 6 } else { 2 }; // magenta held; yellow ready
    frame
}
