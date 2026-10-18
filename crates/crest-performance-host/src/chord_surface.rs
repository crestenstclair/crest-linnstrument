//! Chord layout owns pad assignments, help and LED intent together. Firmware
//! coordinate packets are the only input; firmware LED decoding owns display.
use super::*;
use clip_surface::HarmonyControl as Control;

const DEGREES: [&str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];
// Dense control rows follow Crest Flower's quality/extension and voicing groups.
// Relative columns are contiguous on both devices; see docs/contract.md for the
// source revision and the retained LinnStrument-specific controls.
const CONTROLS: &[(u8, u8, Control, &str)] = &[
    (1, 7, Control::Quality(ChordQuality::Diminished), "Dim"),
    (2, 7, Control::Quality(ChordQuality::Minor), "Min"),
    (3, 7, Control::Quality(ChordQuality::Major), "Maj"),
    (4, 7, Control::Suspension(Suspension::Fourth), "Sus4"),
    (5, 7, Control::Extension(ChordExtension::Sixth), "6"),
    (6, 7, Control::Extension(ChordExtension::MinorSeventh), "m7"),
    (
        7,
        7,
        Control::Extension(ChordExtension::MajorSeventh),
        "Maj7",
    ),
    (8, 7, Control::Extension(ChordExtension::AddNinth), "Add9"),
    (1, 6, Control::InvertDown, "Inv−"),
    (2, 6, Control::InvertReset, "Inv0"),
    (3, 6, Control::InvertUp, "Inv+"),
    (4, 6, Control::VoiceLeading, "Lead"),
    (5, 6, Control::RegisterDown, "Oct−"),
    (6, 6, Control::RegisterReset, "Oct0"),
    (7, 6, Control::RegisterUp, "Oct+"),
    (8, 6, Control::Spread, "Spread"),
    (1, 5, Control::Quality(ChordQuality::Diatonic), "Diat"),
    (2, 5, Control::Quality(ChordQuality::Major), "Chrom"),
    (3, 5, Control::Quality(ChordQuality::Augmented), "Aug"),
    (4, 5, Control::Suspension(Suspension::Second), "Sus2"),
    (5, 5, Control::Extension(ChordExtension::Triad), "Triad"),
    (6, 5, Control::Extension(ChordExtension::Seventh), "7"),
    (7, 5, Control::Extension(ChordExtension::Ninth), "9"),
    (8, 5, Control::Extension(ChordExtension::MajorNinth), "Maj9"),
    (1, 4, Control::Articulation(Articulation::Block), "Block"),
    (
        2,
        4,
        Control::Articulation(Articulation::Strum {
            direction: StrumDirection::Ascending,
            interval: BeatTime(TICKS_PER_BEAT / 16),
        }),
        "Str↑",
    ),
    (
        3,
        4,
        Control::Articulation(Articulation::Strum {
            direction: StrumDirection::Descending,
            interval: BeatTime(TICKS_PER_BEAT / 16),
        }),
        "Str↓",
    ),
    (
        4,
        4,
        Control::Articulation(Articulation::Arpeggio {
            direction: StrumDirection::Ascending,
            interval: BeatTime(TICKS_PER_BEAT / 16),
            gate: BeatTime(TICKS_PER_BEAT / 32),
        }),
        "Arp↑",
    ),
    (
        5,
        4,
        Control::Articulation(Articulation::Arpeggio {
            direction: StrumDirection::Descending,
            interval: BeatTime(TICKS_PER_BEAT / 16),
            gate: BeatTime(TICKS_PER_BEAT / 32),
        }),
        "Arp↓",
    ),
    (
        6,
        4,
        Control::Articulation(Articulation::Humanized {
            max_delay: BeatTime(TICKS_PER_BEAT / 16),
            velocity_variation: 8,
            seed: 42,
        }),
        "Human",
    ),
    (7, 4, Control::Borrowed, "Borrow"),
];
const _: () = assert!(CONTROLS.len() <= u32::BITS as usize);
#[derive(Default)]
pub(super) struct Controls {
    pressed: u32,
}

// A centered 16-column footprint: eight control columns, one separator,
// seven playing columns. Never stretch controls to fill the larger device.
fn control_column(compact: u8, geometry: Geometry) -> u8 {
    compact
        + match geometry {
            Geometry::Pads128 => 0,
            Geometry::Pads200 => 4,
        }
}

fn control_at(
    column: u8,
    row: u8,
    geometry: Geometry,
    settings: HarmonySettings,
) -> Option<(usize, Control, &'static str)> {
    let (index, &(_, _, control, label)) = CONTROLS
        .iter()
        .enumerate()
        .find(|(_, (c, r, _, _))| control_column(*c, geometry) == column && *r == row)?;
    if settings.playing == HarmonyPlaying::Strumplate && matches!(control, Control::Articulation(_))
    {
        return None;
    }
    if settings.quality != ChordQuality::Diatonic && matches!(control, Control::Borrowed) {
        return None;
    }
    Some((index, control, label))
}
/// One I–VII bank immediately beside the control block, with one inert separator.
fn degree(column: u8, geometry: Geometry) -> Option<u8> {
    column
        .checked_sub(control_column(1, geometry) + 9)
        .filter(|degree| *degree < 7)
}
fn key_at(column: u8, row: u8, geometry: Geometry, settings: HarmonySettings) -> Option<u8> {
    if row == 0 {
        return None;
    }
    let degree = degree(column, geometry)?;
    let step = if settings.playing == HarmonyPlaying::Strumplate {
        if row == 7 {
            return None;
        }
        let voicing = settings.voicing(settings.degree, None).ok()?;
        (i16::from(row) - 5) * voicing.notes().len() as i16 + i16::from(degree)
    } else {
        (i16::from(row) - 5) * 7 + i16::from(degree)
    };
    let key = i16::from(settings.input_anchor) + step;
    let key = u8::try_from(key).ok().filter(|k| *k <= 127)?;
    if settings.playing == HarmonyPlaying::Strumplate {
        settings
            .voicing(settings.degree, None)
            .ok()?
            .strumplate_pitch(step)
            .ok()?;
    } else {
        let v = settings
            .voicing_at_octave(degree, i16::from(row) - 5, None)
            .ok()?;
        if settings.bass_track.is_some() && v.root() < 12 {
            return None;
        }
    }
    Some(key)
}
fn pad_help(column: u8, row: u8, geometry: Geometry, settings: HarmonySettings) -> Option<PadHelp> {
    if let Some((_, control, label)) = control_at(column, row, geometry, settings) {
        let mut help = PadHelp::new(column, row, control.title(), control.description(settings));
        help.label = label.into();
        return Some(help);
    }
    let degree = degree(column, geometry)?;
    if settings.playing == HarmonyPlaying::Strumplate && row == 7 {
        settings.voicing(degree, None).ok()?;
        let mut help = PadHelp::new(
            column,
            row,
            format!("Select chord {}", DEGREES[usize::from(degree)]),
            "Select harmony for future manual picks. Held notes retain their pitches.",
        );
        help.label = DEGREES[usize::from(degree)].into();
        return Some(help);
    }
    let key = key_at(column, row, geometry, settings)?;
    let (title, label) = if settings.playing == HarmonyPlaying::Strumplate {
        let pitch = settings
            .voicing(settings.degree, None)
            .ok()?
            .strumplate_pitch(i16::from(key) - i16::from(settings.input_anchor))
            .ok()?;
        (
            format!("Pick {} · MIDI {pitch}", note_name(pitch)),
            note_name(pitch),
        )
    } else {
        let voicing = settings
            .voicing_at_octave(degree, i16::from(row) - 5, None)
            .ok()?;
        (
            format!(
                "Chord {} · {} · {} {:?}",
                DEGREES[usize::from(degree)],
                note_name(voicing.root()),
                if settings.voice_leading {
                    "base notes; Lead may change octaves"
                } else {
                    "notes"
                },
                voicing.notes()
            ),
            format!(
                "{} {}",
                DEGREES[usize::from(degree)],
                i16::from(settings.tonic) / 12 - 1 + i16::from(row) - 5
                    + i16::from(settings.register)
            ),
        )
    };
    let mut help = PadHelp::new(
        column,
        row,
        title,
        "Press to play. In-cell X bends this contact; Y controls timbre; pressure controls its voices. Crossing a cell boundary retriggers. Release ends its notes. Root/key is selected above the instrument.",
    );
    help.label = label;
    Some(help)
}
fn note_name(pitch: u8) -> String {
    format!(
        "{}{}",
        [
            "C", "C♯", "D", "E♭", "E", "F", "F♯", "G", "A♭", "A", "B♭", "B"
        ][usize::from(pitch % 12)],
        i16::from(pitch) / 12 - 1
    )
}
pub(super) fn help(settings: HarmonySettings, geometry: Geometry) -> Vec<PadHelp> {
    (1..geometry.columns())
        .flat_map(|column| column_help(column, geometry, settings))
        .collect()
}
fn column_help(
    column: u8,
    geometry: Geometry,
    settings: HarmonySettings,
) -> impl Iterator<Item = PadHelp> {
    (0..8).filter_map(move |row| pad_help(column, row, geometry, settings))
}

pub(super) fn frame(
    settings: HarmonySettings,
    geometry: Geometry,
    input: &UserNoteInput,
) -> [[u8; 8]; 26] {
    let mut frame = [[0; 8]; 26];
    for column in 1..geometry.columns() {
        frame[usize::from(column)] = column_colors(column, settings, geometry, input);
    }
    frame
}
fn column_colors(
    column: u8,
    settings: HarmonySettings,
    geometry: Geometry,
    input: &UserNoteInput,
) -> [u8; 8] {
    std::array::from_fn(|row| {
        let row = row as u8;
        if input.held(column, row) {
            return 8;
        }
        if let Some((_, control, _)) = control_at(column, row, geometry, settings) {
            if column == control_column(5, geometry)
                && row == 7
                && settings.quality != ChordQuality::Diatonic
            {
                return 3;
            }
            return control.color(settings);
        }
        let selector = settings.playing == HarmonyPlaying::Strumplate
            && row == 7
            && degree(column, geometry)
                .is_some_and(|degree| settings.voicing(degree, None).is_ok());
        if !selector && key_at(column, row, geometry, settings).is_none() {
            return 0;
        }
        if degree(column, geometry) == Some(settings.degree) {
            3
        } else {
            5
        }
    })
}

impl TestHost {
    pub(super) fn chord_grid(&self) -> bool {
        self.mode == SurfaceMode::Chords && self.engine.session().harmony.input_octaves
    }
    pub(super) fn accept_chord_message(
        &mut self,
        at: BeatTime,
        message: MidiMessage,
    ) -> Result<()> {
        let settings = self.engine.session().harmony;
        let pad = user_pad(message, self.geometry)?;
        if let Some(pad) = pad {
            self.chord_control(pad, settings)?;
        }
        let key = pad
            .filter(|p| p.pressed)
            .and_then(|p| key_at(p.column, p.row, self.geometry, settings));
        if let Some(event) = self.chord_input.accept(at, message, key)?
            && let Err(error) = self.engine.input(event)
        {
            if matches!(event.gesture, Gesture::On { .. })
                && let Some(pad) = pad
            {
                self.chord_input.reject(pad.column, pad.row);
            }
            return Err(error);
        }
        Ok(())
    }
    fn chord_control(&mut self, pad: PadEvent, settings: HarmonySettings) -> Result<()> {
        if !pad.pressed {
            if let Some(index) = CONTROLS.iter().position(|(c, r, _, _)| {
                control_column(*c, self.geometry) == pad.column && *r == pad.row
            }) {
                self.chord_controls.pressed &= !(1 << index);
            }
            return Ok(());
        }
        if let Some((index, control, _)) = control_at(pad.column, pad.row, self.geometry, settings)
        {
            let mask = 1 << index;
            if self.chord_controls.pressed & mask != 0 {
                return Ok(());
            }
            self.chord_controls.pressed |= mask;
            if let Ok(next) = control.change(settings) {
                self.engine.command(Command::SetHarmony(next))?;
            }
        } else if settings.playing == HarmonyPlaying::Strumplate
            && pad.row == 7
            && pad.pressed
            && let Some(degree) = degree(pad.column, self.geometry)
        {
            let next = HarmonySettings { degree, ..settings };
            if next.validate().is_ok() {
                self.engine.command(Command::SetHarmony(next))?;
            }
        }
        Ok(())
    }
}
