//! Deterministic diatonic harmony. Pitches are rendered at attack; later settings
//! changes never retune held voices. No MIDI channel or device geometry is used.
use crate::{BeatTime, Error, InputEvent, Result, TICKS_PER_BEAT, TrackId};
use serde::{Deserialize, Serialize};

pub const HARMONY_ALGORITHM_VERSION: u32 = 2;
pub const MAX_CHORD_VOICES: usize = 6;
/// Reserved for engine-rendered notes. External input must use another source.
pub const GENERATED_SOURCE: crate::SourceId = crate::SourceId(u32::MAX);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    #[default]
    Major,
    NaturalMinor,
    Dorian,
    Mixolydian,
}
impl Scale {
    fn intervals(self) -> [i16; 7] {
        match self {
            Self::Major => [0, 2, 4, 5, 7, 9, 11],
            Self::NaturalMinor => [0, 2, 3, 5, 7, 8, 10],
            Self::Dorian => [0, 2, 3, 5, 7, 9, 10],
            Self::Mixolydian => [0, 2, 4, 5, 7, 9, 10],
        }
    }
    fn tone(self, degree: i16) -> i16 {
        self.intervals()[degree.rem_euclid(7) as usize] + 12 * degree.div_euclid(7)
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChordExtension {
    #[default]
    Triad,
    Seventh,
    Ninth,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Suspension {
    #[default]
    None,
    Second,
    Fourth,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpressionPolicy {
    #[default]
    AllVoices,
    TopVoice,
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarmonyPlaying {
    #[default]
    ChordPads,
    Strumplate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StrumDirection {
    Ascending,
    Descending,
}

/// A finite sweep of the held chord. Releasing its trigger cancels tones that
/// have not sounded. Bass and manually picked strumplate notes attack immediately.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Articulation {
    #[default]
    Block,
    Strum {
        direction: StrumDirection,
        interval: BeatTime,
    },
}
impl Articulation {
    pub fn validate(self) -> Result<()> {
        if let Self::Strum { interval, .. } = self
            && !(1..=TICKS_PER_BEAT).contains(&interval.0)
        {
            return Err(Error::Invalid(
                "strum interval must be 1 tick through 1 beat",
            ));
        }
        Ok(())
    }
    pub(crate) fn delay(self, index: usize, count: usize) -> BeatTime {
        match self {
            Self::Block => BeatTime(0),
            Self::Strum {
                direction,
                interval,
            } => {
                let step = match direction {
                    StrumDirection::Ascending => index,
                    StrumDirection::Descending => count - 1 - index,
                };
                BeatTime(interval.0 * step as u64)
            }
        }
    }
}

/// `tonic` is a MIDI note, not only a pitch class. Seven adjacent chromatic
/// input keys beginning at `input_anchor` select degrees I–VII, repeating.
/// Strumplate maps those input steps to successive voiced chord tones instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarmonySettings {
    pub tonic: u8,
    pub scale: Scale,
    pub degree: u8,
    pub extension: ChordExtension,
    pub suspension: Suspension,
    pub borrowed: bool,
    pub inversion: i8,
    pub spread: bool,
    pub register: i8,
    pub voice_leading: bool,
    pub expression: ExpressionPolicy,
    pub playing: HarmonyPlaying,
    #[serde(default)]
    pub articulation: Articulation,
    pub input_anchor: u8,
    pub bass_track: Option<TrackId>,
}
impl Default for HarmonySettings {
    fn default() -> Self {
        Self {
            tonic: 60,
            scale: Scale::Major,
            degree: 0,
            extension: ChordExtension::Triad,
            suspension: Suspension::None,
            borrowed: false,
            inversion: 0,
            spread: false,
            register: 0,
            voice_leading: false,
            expression: ExpressionPolicy::AllVoices,
            playing: HarmonyPlaying::ChordPads,
            articulation: Articulation::Block,
            input_anchor: 30,
            bass_track: None,
        }
    }
}
impl HarmonySettings {
    pub fn validate(self) -> Result<()> {
        self.articulation.validate()?;
        if self.tonic > 127
            || self.input_anchor > 127
            || self.degree >= 7
            || !(-4..=4).contains(&self.inversion)
            || !(-3..=3).contains(&self.register)
        {
            return Err(Error::Invalid("harmony settings"));
        }
        self.voicing(self.degree, None)?;
        Ok(())
    }
    /// Bounded, allocation-free voicing. Rejects out-of-range notes rather than
    /// clamping or silently dropping voices. Voice leading minimizes sorted
    /// voice movement over octave placements and inversions; ties retain the
    /// requested inversion. A changed chord size starts from requested voicing.
    pub fn voicing(self, degree: u8, previous: Option<&Voicing>) -> Result<Voicing> {
        if degree >= 7
            || self.tonic > 127
            || !(-3..=3).contains(&self.register)
            || !(-4..=4).contains(&self.inversion)
        {
            return Err(Error::Invalid("chord degree or register"));
        }
        let scale = if self.borrowed {
            if self.scale == Scale::NaturalMinor {
                Scale::Major
            } else {
                Scale::NaturalMinor
            }
        } else {
            self.scale
        };
        let root = i16::from(self.tonic) + 12 * i16::from(self.register);
        let count = match self.extension {
            ChordExtension::Triad => 3,
            ChordExtension::Seventh => 4,
            ChordExtension::Ninth => 5,
        };
        let mut notes = [0i16; 5];
        for (index, note) in notes.iter_mut().take(count).enumerate() {
            let step = if index == 1 {
                match self.suspension {
                    Suspension::None => 2,
                    Suspension::Second => 1,
                    Suspension::Fourth => 3,
                }
            } else {
                2 * index as i16
            };
            *note = root + scale.tone(i16::from(degree) + step);
        }
        invert(&mut notes[..count], self.inversion);
        if self.spread {
            notes[1] += 12;
            notes[..count].sort_unstable();
        }
        if self.voice_leading
            && let Some(previous) = previous.filter(|p| p.notes().len() == count)
        {
            notes = nearest_voicing(notes, count, previous);
        }
        let mut result = Voicing {
            notes: [0; 5],
            count: count as u8,
            root: 0,
        };
        for (target, pitch) in result.notes.iter_mut().zip(notes).take(count) {
            *target = midi_pitch(pitch)?;
        }
        result.root = midi_pitch(root + scale.tone(i16::from(degree)))?;
        Ok(result)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarmonyGesture {
    pub algorithm_version: u32,
    pub input: InputEvent,
    pub settings: HarmonySettings,
}
impl HarmonyGesture {
    pub fn validate(&self) -> Result<()> {
        if self.algorithm_version != HARMONY_ALGORITHM_VERSION
            && !(self.algorithm_version == 1 && self.settings.articulation == Articulation::Block)
        {
            return Err(Error::Version(self.algorithm_version));
        }
        self.input.gesture.validate()?;
        self.settings.validate()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Voicing {
    notes: [u8; 5],
    count: u8,
    root: u8,
}
impl Voicing {
    pub fn notes(&self) -> &[u8] {
        &self.notes[..usize::from(self.count)]
    }
    pub fn root(&self) -> u8 {
        self.root
    }
    pub fn strumplate_pitch(&self, step: i16) -> Result<u8> {
        let count = i16::from(self.count);
        midi_pitch(
            i16::from(self.notes[step.rem_euclid(count) as usize]) + 12 * step.div_euclid(count),
        )
    }
}
fn midi_pitch(pitch: i16) -> Result<u8> {
    if !(0..=127).contains(&pitch) {
        return Err(Error::Invalid("voicing outside MIDI range"));
    }
    Ok(pitch as u8)
}
fn invert(notes: &mut [i16], inversion: i8) {
    for _ in 0..inversion.unsigned_abs() {
        if inversion >= 0 {
            notes[0] += 12;
        } else {
            *notes.last_mut().expect("nonempty chord") -= 12;
        }
        notes.sort_unstable();
    }
}
fn movement(notes: &[i16], previous: &Voicing) -> u16 {
    notes
        .iter()
        .zip(previous.notes())
        .map(|(a, b)| (a - i16::from(*b)).unsigned_abs())
        .sum()
}
fn nearest_voicing(original: [i16; 5], count: usize, previous: &Voicing) -> [i16; 5] {
    let mut best = original;
    let mut cost = u16::MAX;
    if original[..count].iter().all(|p| (0..=127).contains(p)) {
        cost = movement(&original[..count], previous);
    }
    // Each candidate is a different rotation, transposed by up to two octaves.
    for candidate in 0..count * 5 {
        let mut notes = original;
        invert(&mut notes[..count], (candidate % count) as i8);
        for_pitch_shift(&mut notes[..count], (candidate / count) as i16 * 12 - 24);
        if notes[..count].iter().all(|p| (0..=127).contains(p))
            && movement(&notes[..count], previous) < cost
        {
            cost = movement(&notes[..count], previous);
            best = notes;
        }
    }
    best
}
fn for_pitch_shift(notes: &mut [i16], shift: i16) {
    for note in notes {
        *note += shift;
    }
}
