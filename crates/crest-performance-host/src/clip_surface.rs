//! Standalone host's stock User Firmware Mode layout. Both OS MIDI and the
//! simulator enter through the same decoded pad events; LEDs remain intent.
use super::*;

#[derive(Default)]
pub(super) struct Controls {
    pressed_voicing: u8,
}
impl Controls {
    /// Only a fresh press changes voicing. Releases and duplicate MIDI attacks
    /// cannot step again. Unavailable steps are dark and inert until released.
    fn voicing_change(
        &mut self,
        pad: PadEvent,
        settings: HarmonySettings,
    ) -> Option<HarmonySettings> {
        let index = usize::from(pad.column.checked_sub(2)?);
        let control = *VOICING_CONTROLS.get(index)?;
        let mask = 1 << index;
        if !pad.pressed {
            self.pressed_voicing &= !mask;
            return None;
        }
        if self.pressed_voicing & mask != 0 {
            return None;
        }
        self.pressed_voicing |= mask;
        control.change(settings).ok()
    }
}

#[derive(Clone, Copy)]
enum VoicingControl {
    InvertDown,
    InvertUp,
    RegisterDown,
    RegisterUp,
    Spread,
    VoiceLeading,
}
const VOICING_CONTROLS: [VoicingControl; 6] = [
    VoicingControl::InvertDown,
    VoicingControl::InvertUp,
    VoicingControl::RegisterDown,
    VoicingControl::RegisterUp,
    VoicingControl::Spread,
    VoicingControl::VoiceLeading,
];
impl VoicingControl {
    fn change(self, mut settings: HarmonySettings) -> Result<HarmonySettings> {
        match self {
            Self::InvertDown => settings.inversion -= 1,
            Self::InvertUp => settings.inversion += 1,
            Self::RegisterDown => settings.register -= 1,
            Self::RegisterUp => settings.register += 1,
            Self::Spread => settings.spread = !settings.spread,
            Self::VoiceLeading => settings.voice_leading = !settings.voice_leading,
        }
        settings.validate()?;
        Ok(settings)
    }
    fn color(self, settings: HarmonySettings) -> u8 {
        if self.change(settings).is_err() {
            return 0; // unavailable step: do not wrap, clamp or drop chord tones
        }
        match self {
            Self::Spread if settings.spread => 3,
            Self::VoiceLeading if settings.voice_leading => 3,
            _ => 5, // blue step/toggle; enabled toggles are green
        }
    }
}

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
            6 => {
                let effective = self.engine.snapshot().effective_harmony;
                if let Some(settings) = self.clip_controls.voicing_change(pad, effective) {
                    // Take over the audible chord, preserving its degree and
                    // musical identity even while a recorded leader is running.
                    self.engine.command(Command::SetHarmony(settings))?;
                    self.engine
                        .command(Command::SetHarmonySource(HarmonySource::Selected {}))?;
                }
            }
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
    for (index, control) in VOICING_CONTROLS.iter().enumerate() {
        frame[index + 2][6] = control.color(state.effective_harmony);
    }
    frame
}
