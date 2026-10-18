//! Standalone host's stock User Firmware Mode layout. Both OS MIDI and the
//! simulator enter through the same decoded pad events; LEDs remain intent.
use super::*;

#[derive(Debug, Clone, Serialize)]
pub struct PadHelp {
    pub label: String,
    pub column: u8,
    pub row: u8,
    pub title: String,
    pub action: String,
}

#[derive(Default)]
pub(super) struct Controls {
    pressed_harmony: u32,
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
pub(super) enum HarmonyControl {
    InvertDown,
    InvertReset,
    RegisterReset,
    Quality(ChordQuality),
    InvertUp,
    RegisterDown,
    RegisterUp,
    Spread,
    VoiceLeading,
    Extension(ChordExtension),
    Suspension(Suspension),
    Borrowed,
    Articulation(Articulation),
}
const HARMONY_CONTROLS: [(u8, u8, HarmonyControl); 17] = [
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
    (5, 6, HarmonyControl::Articulation(Articulation::Block)),
    (
        5,
        7,
        HarmonyControl::Articulation(Articulation::Strum {
            direction: StrumDirection::Ascending,
            interval: BeatTime(TICKS_PER_BEAT / 16),
        }),
    ),
    (
        5,
        8,
        HarmonyControl::Articulation(Articulation::Strum {
            direction: StrumDirection::Descending,
            interval: BeatTime(TICKS_PER_BEAT / 16),
        }),
    ),
    (
        5,
        9,
        HarmonyControl::Articulation(Articulation::Arpeggio {
            direction: StrumDirection::Ascending,
            interval: BeatTime(TICKS_PER_BEAT / 16),
            gate: BeatTime(TICKS_PER_BEAT / 32),
        }),
    ),
    (
        5,
        10,
        HarmonyControl::Articulation(Articulation::Arpeggio {
            direction: StrumDirection::Descending,
            interval: BeatTime(TICKS_PER_BEAT / 16),
            gate: BeatTime(TICKS_PER_BEAT / 32),
        }),
    ),
    (
        5,
        11,
        HarmonyControl::Articulation(Articulation::Humanized {
            max_delay: BeatTime(TICKS_PER_BEAT / 16),
            velocity_variation: 8,
            seed: 42,
        }),
    ),
];
const _: () = assert!(HARMONY_CONTROLS.len() <= u32::BITS as usize);
impl HarmonyControl {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::InvertDown => "Previous inversion",
            Self::InvertReset => "Root position",
            Self::RegisterReset => "Reset register",
            Self::Quality(ChordQuality::Diatonic) => "Diatonic",
            Self::Quality(ChordQuality::Major) => "Major / Chromatic",
            Self::Quality(ChordQuality::Minor) => "Minor",
            Self::Quality(ChordQuality::Diminished) => "Diminished",
            Self::Quality(ChordQuality::Augmented) => "Augmented",
            Self::InvertUp => "Next inversion",
            Self::RegisterDown => "Lower chord register",
            Self::RegisterUp => "Raise chord register",
            Self::Spread => "Open chord spread",
            Self::VoiceLeading => "Voice leading",
            Self::Extension(ChordExtension::Seventh) => "Seventh chord",
            Self::Extension(ChordExtension::Ninth) => "Ninth chord",
            Self::Extension(ChordExtension::Triad) => "Triad",
            Self::Extension(ChordExtension::AddNinth) => "Add9 (no seventh)",
            Self::Extension(ChordExtension::Sixth) => "Sixth",
            Self::Extension(ChordExtension::MajorSeventh) => "Major seventh",
            Self::Extension(ChordExtension::MinorSeventh) => "Minor seventh",
            Self::Extension(ChordExtension::MajorNinth) => "Major ninth",
            Self::Suspension(Suspension::Second) => "Suspended second",
            Self::Suspension(Suspension::Fourth) => "Suspended fourth",
            Self::Suspension(Suspension::None) => "No suspension",
            Self::Borrowed => "Borrow parallel minor / major",
            Self::Articulation(Articulation::Block) => "Play chord together",
            Self::Articulation(Articulation::Strum {
                direction: StrumDirection::Ascending,
                ..
            }) => "Strum low to high",
            Self::Articulation(Articulation::Strum {
                direction: StrumDirection::Descending,
                ..
            }) => "Strum high to low",
            Self::Articulation(Articulation::Arpeggio {
                direction: StrumDirection::Ascending,
                ..
            }) => "Arpeggio low to high",
            Self::Articulation(Articulation::Arpeggio {
                direction: StrumDirection::Descending,
                ..
            }) => "Arpeggio high to low",
            Self::Articulation(Articulation::Humanized { .. }) => "Humanized chord",
        }
    }

    pub(super) fn description(self, settings: HarmonySettings) -> String {
        let Ok(next) = self.change(settings) else {
            return "Unavailable for the current voicing. This pad will not change it.".into();
        };
        let action = match self {
            Self::InvertDown | Self::InvertUp | Self::InvertReset => format!("Inversion {}. Press to select inversion {}: move a chord tone into the neighboring octave.", settings.inversion, next.inversion),
            Self::RegisterDown | Self::RegisterUp | Self::RegisterReset => format!("Register {:+} octaves. Press to shift the whole chord to {:+} octaves.", settings.register, next.register),
            Self::Quality(_) => "Select this chord quality on the chosen scale-degree roots. Diatonic derives each quality from the scale; explicit qualities use fixed semitone intervals.".into(),
            Self::Spread => if next.spread { "Press to raise the second chord tone one octave, opening the voicing." } else { "Press to restore close chord spacing." }.into(),
            Self::VoiceLeading => if next.voice_leading { "Press to choose nearby octave placements between successive chords." } else { "Press to use the requested inversion without automatic voice leading." }.into(),
            Self::Extension(_) => if next.extension == ChordExtension::Triad { "Press to remove the extension and return to a triad." } else { "Press to use this chord extension. Selecting it again returns to a triad." }.into(),
            Self::Suspension(_) => if next.suspension == Suspension::None { "Press to restore the chord third." } else { "Press to replace the third with this suspension. Selecting it again restores the third." }.into(),
            Self::Borrowed => if next.borrowed { "Press to borrow this degree from the parallel minor or major scale." } else { "Press to restore this degree from the selected scale." }.into(),
            Self::Articulation(Articulation::Block) => "Press to start every chord tone together.".into(),
            Self::Articulation(Articulation::Strum { .. }) => "Press to start chord tones once in this direction, separated by the configured strum interval.".into(),
            Self::Articulation(Articulation::Arpeggio { .. }) => "Press to repeat chord tones in this direction while the chord pad is held. Releasing it ends the arpeggio.".into(),
            Self::Articulation(Articulation::Humanized { .. }) => "Press to vary chord-tone timing and velocity using the configured seed and limits.".into(),
        };
        let status = if self.color(settings) == 3 {
            "Selected / enabled (green)."
        } else {
            "Available (blue)."
        };
        format!(
            "{status} {action} Changes affect future chord attacks; held notes keep their pitches."
        )
    }

    pub(super) fn change(self, mut settings: HarmonySettings) -> Result<HarmonySettings> {
        // Learned entries are fixed pitch sets; diatonic modifiers would be
        // inaudible. Keep those pads dark until the degree is diatonic again.
        if matches!(
            self,
            Self::Extension(_) | Self::Suspension(_) | Self::Borrowed | Self::Quality(_)
        ) && settings.learned[usize::from(settings.degree)].is_some()
        {
            return Err(Error::State("learned voicing keeps fixed pitches"));
        }
        match self {
            Self::InvertDown => settings.inversion -= 1,
            Self::InvertReset => settings.inversion = 0,
            Self::RegisterReset => settings.register = 0,
            Self::Quality(quality) => settings.quality = quality,
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
            Self::Articulation(choice) => {
                settings.articulation = select_articulation(choice, settings.articulation);
            }
        }
        settings.validate()?;
        Ok(settings)
    }
    pub(super) fn color(self, settings: HarmonySettings) -> u8 {
        if self.change(settings).is_err() {
            return 0; // unavailable step: do not wrap, clamp or drop chord tones
        }
        match self {
            Self::Quality(quality) if settings.quality == quality => 3,
            Self::InvertReset if settings.inversion == 0 => 3,
            Self::RegisterReset if settings.register == 0 => 3,
            Self::Spread if settings.spread => 3,
            Self::VoiceLeading if settings.voice_leading => 3,
            Self::Extension(extension) if settings.extension == extension => 3,
            Self::Suspension(suspension) if settings.suspension == suspension => 3,
            Self::Borrowed if settings.borrowed => 3,
            Self::Articulation(choice)
                if select_articulation(choice, settings.articulation) == settings.articulation =>
            {
                3
            }
            _ => 5, // blue step/toggle; enabled toggles are green
        }
    }
}

impl PadHelp {
    pub(super) fn new(
        column: u8,
        row: u8,
        title: impl Into<String>,
        action: impl Into<String>,
    ) -> Self {
        Self {
            label: String::new(),
            column,
            row,
            title: title.into(),
            action: action.into(),
        }
    }
}

fn octave_help(slot: usize, row: u8, direction: &str, running: bool, shift: i8) -> PadHelp {
    let action = if running {
        format!(
            "Hold to shift future attacks one octave {direction}. Release restores original pitches. Blue: ready. Magenta: active. Current shift: {shift} semitones."
        )
    } else {
        "Unavailable while this clip is stopped. Launch it first.".into()
    };
    PadHelp::new(
        slot as u8 + 1,
        row,
        format!("Hold octave {direction} · clip {}", slot + 1),
        action,
    )
}

fn clip_help(state: &PerformanceSnapshot, slot: usize, status: SlotState) -> [PadHelp; 4] {
    let column = slot as u8 + 1;
    let running = matches!(status, SlotState::Playing | SlotState::Queued);
    let action = match status {
        SlotState::Empty => "Empty. Record a phrase into this slot before launching it.",
        SlotState::Recorded => "Recorded (blue). Press to launch this clip using the launch grid.",
        SlotState::Queued => "Queued (yellow). Press to cancel the pending launch.",
        SlotState::Playing if state.muted_slots[slot] => {
            "Muted (cyan). The loop keeps running. Release its mute hold to restore future attacks; press this pad to stop."
        }
        SlotState::Playing => "Playing (green). Press to stop this clip and release its voices.",
        SlotState::Recording => "Recording (red). Finish the take before launching it.",
        SlotState::Overdubbing => "Overdubbing (magenta). Finish the take to keep the added notes.",
    };
    let mute = if running {
        "Hold to silence this clip while its loop continues; release restores future attacks. Blue: available. Cyan: muted."
    } else {
        "Unavailable while this clip is stopped. Launch it first."
    };
    [
        PadHelp::new(column, 0, format!("Clip {}", slot + 1), action),
        PadHelp::new(column, 1, format!("Hold mute · clip {}", slot + 1), mute),
        octave_help(slot, 3, "down", running, state.clip_transpositions[slot]),
        octave_help(slot, 4, "up", running, state.clip_transpositions[slot]),
    ]
}

/// The same control definitions own actions, LED intent and user explanations.
/// The simulator must still draw its decoded firmware LED frame separately.
pub(super) fn help(state: &PerformanceSnapshot, geometry: Geometry, morph: bool) -> Vec<PadHelp> {
    if state.mode != SurfaceMode::Clips {
        return Vec::new();
    }
    let mut result = Vec::new();
    for (slot, status) in state.slots.iter().take(25).enumerate() {
        result.extend(clip_help(state, slot, *status));
    }
    for (degree, numeral) in ["I", "II", "III", "IV", "V", "VI", "VII"]
        .into_iter()
        .enumerate()
    {
        let selected = if state.effective_harmony.degree == degree as u8 {
            "Selected (green)"
        } else {
            "Available (blue)"
        };
        result.push(PadHelp::new(degree as u8+1, 7, format!("Harmony degree {numeral}"), format!(
            "{selected}. Press to select this chord for followers and Strumplate; it does not itself sound a chord. Return to Chords to play it. Held notes keep their pitches."
        )));
    }
    let fill = if state.fill {
        "Held (magenta)"
    } else {
        "Ready (yellow)"
    };
    result.push(PadHelp::new(1, 6, "Hold Fill", format!("{fill}. Hold to enable Fill-conditioned attacks; release restores the normal conditions.")));
    for (row, column, control) in HARMONY_CONTROLS {
        result.push(PadHelp::new(
            column,
            row,
            control.title(),
            control.description(state.effective_harmony),
        ));
    }
    let morph_action = if morph {
        "Slide across this row to interpolate the configured parameters. Blue: available. Green: current position. Release keeps the last value. The standalone backend captures values only; no audible effect is connected."
    } else {
        "No A/B endpoints configured. This row is inactive."
    };
    for column in 1..geometry.columns() {
        result.push(PadHelp::new(column, 2, "A/B morph strip", morph_action));
    }
    result.retain(|pad| pad.column < geometry.columns());
    result
}

/// Keep custom timing within an articulation family, including direction
/// changes. Entering another family selects the documented controller preset.
/// Selecting Humanized again retains its seed and variation settings.
fn select_articulation(choice: Articulation, current: Articulation) -> Articulation {
    match (choice, current) {
        (Articulation::Strum { direction, .. }, Articulation::Strum { interval, .. }) => {
            Articulation::Strum {
                direction,
                interval,
            }
        }
        (
            Articulation::Arpeggio { direction, .. },
            Articulation::Arpeggio { interval, gate, .. },
        ) => Articulation::Arpeggio {
            direction,
            interval,
            gate,
        },
        (Articulation::Humanized { .. }, Articulation::Humanized { .. }) => current,
        _ => choice,
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
            3 | 4 if index < self.engine.session().slots.len() => {
                let state = self.engine.snapshot().slots[index];
                if !pad.pressed || matches!(state, SlotState::Playing | SlotState::Queued) {
                    let direction = if pad.row == 3 {
                        OctaveDirection::Down
                    } else {
                        OctaveDirection::Up
                    };
                    self.set_octave(ControlSource::Pad, index, direction, pad.pressed)?;
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
            let transpose = state.clip_transpositions[slot];
            frame[slot + 1][3] = if transpose < 0 { 6 } else { 5 };
            frame[slot + 1][4] = if transpose > 0 { 6 } else { 5 };
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
