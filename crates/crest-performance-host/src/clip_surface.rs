//! Standalone host's stock User Firmware Mode layout. Both OS MIDI and the
//! simulator enter through the same decoded pad events; LEDs remain intent.
use super::*;

#[derive(Default)]
pub(super) struct Controls {
    pressed_harmony: u16,
}
impl Controls {
    /// Only a fresh press changes harmony. Releases and duplicate MIDI attacks
    /// cannot step again. Unavailable steps are dark and inert until released.
    fn harmony_change(
        &mut self,
        pad: PadEvent,
        settings: HarmonySettings,
    ) -> Option<HarmonySettings> {
        let index = HARMONY_CONTROLS
            .iter()
            .position(|(row, column, _)| *row == pad.row && *column == pad.column)?;
        let (_, _, control) = HARMONY_CONTROLS[index];
        let mask = 1 << index;
        if !pad.pressed {
            self.pressed_harmony &= !mask;
            return None;
        }
        if self.pressed_harmony & mask != 0 {
            return None;
        }
        self.pressed_harmony |= mask;
        control.change(settings).ok()
    }
}

#[derive(Clone, Copy)]
enum HarmonyControl {
    InvertDown,
    InvertUp,
    RegisterDown,
    RegisterUp,
    Spread,
    VoiceLeading,
    Extension(ChordExtension),
    Suspension(Suspension),
    Borrowed,
}
const HARMONY_CONTROLS: [(u8, u8, HarmonyControl); 11] = [
    (6, 2, HarmonyControl::InvertDown),
    (6, 3, HarmonyControl::InvertUp),
    (6, 4, HarmonyControl::RegisterDown),
    (6, 5, HarmonyControl::RegisterUp),
    (6, 6, HarmonyControl::Spread),
    (6, 7, HarmonyControl::VoiceLeading),
    (5, 1, HarmonyControl::Extension(ChordExtension::Seventh)),
    (5, 2, HarmonyControl::Extension(ChordExtension::Ninth)),
    (5, 3, HarmonyControl::Suspension(Suspension::Second)),
    (5, 4, HarmonyControl::Suspension(Suspension::Fourth)),
    (5, 5, HarmonyControl::Borrowed),
];
impl HarmonyControl {
    fn change(self, mut settings: HarmonySettings) -> Result<HarmonySettings> {
        // Learned entries are fixed pitch sets; diatonic modifiers would be
        // inaudible. Keep those pads dark until the degree is diatonic again.
        if matches!(
            self,
            Self::Extension(_) | Self::Suspension(_) | Self::Borrowed
        ) && settings.learned[usize::from(settings.degree)].is_some()
        {
            return Err(Error::State("learned voicing keeps fixed pitches"));
        }
        match self {
            Self::InvertDown => settings.inversion -= 1,
            Self::InvertUp => settings.inversion += 1,
            Self::RegisterDown => settings.register -= 1,
            Self::RegisterUp => settings.register += 1,
            Self::Spread => settings.spread = !settings.spread,
            Self::VoiceLeading => settings.voice_leading = !settings.voice_leading,
            Self::Extension(extension) => {
                settings.extension = if settings.extension == extension {
                    ChordExtension::Triad
                } else {
                    extension
                };
            }
            Self::Suspension(suspension) => {
                settings.suspension = if settings.suspension == suspension {
                    Suspension::None
                } else {
                    suspension
                };
            }
            Self::Borrowed => settings.borrowed = !settings.borrowed,
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
            Self::Extension(extension) if settings.extension == extension => 3,
            Self::Suspension(suspension) if settings.suspension == suspension => 3,
            Self::Borrowed if settings.borrowed => 3,
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
            5 | 6 => {
                let effective = self.engine.snapshot().effective_harmony;
                if let Some(settings) = self.clip_controls.harmony_change(pad, effective) {
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
    for (row, column, control) in HARMONY_CONTROLS {
        frame[usize::from(column)][usize::from(row)] = control.color(state.effective_harmony);
    }
    frame
}
