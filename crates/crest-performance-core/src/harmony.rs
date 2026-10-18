//! Deterministic diatonic harmony. Pitches are rendered at attack; later settings
//! changes never retune held voices. No MIDI channel or device geometry is used.
use crate::{
    BeatTime, Error, InputEvent, LearnedChord, MAX_VOICING_NOTES, Result, TICKS_PER_BEAT, TrackId,
};
use serde::{Deserialize, Serialize};

pub const HARMONY_ALGORITHM_VERSION: u32 = 6;
pub const MAX_CHORD_VOICES: usize = MAX_VOICING_NOTES + 1;
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
    AddNinth,
    Sixth,
    MajorSeventh,
    MinorSeventh,
    MajorNinth,
}
/// Diatonic stacks the selected scale. Explicit qualities use chromatic
/// intervals above that same scale-degree root.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChordQuality {
    #[default]
    Diatonic,
    Major,
    Minor,
    Diminished,
    Augmented,
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

/// Held-chord articulation. Releasing the trigger cancels future attacks and
/// releases sounding voices. Bass and manually picked strumplate notes stay immediate.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Articulation {
    #[default]
    Block,
    Strum {
        direction: StrumDirection,
        interval: BeatTime,
    },
    Arpeggio {
        direction: StrumDirection,
        interval: BeatTime,
        gate: BeatTime,
    },
    /// Seeded positive attack jitter and symmetric velocity variation. Each
    /// trigger's identity selects a repeatable variation, without mutable RNG.
    Humanized {
        max_delay: BeatTime,
        velocity_variation: u8,
        seed: u32,
    },
}
impl Articulation {
    pub fn validate(self) -> Result<()> {
        match self {
            Self::Block => Ok(()),
            Self::Strum { interval, .. } | Self::Arpeggio { interval, .. }
                if !(1..=TICKS_PER_BEAT).contains(&interval.0) =>
            {
                Err(Error::Invalid(
                    "articulation interval must be 1 tick through 1 beat",
                ))
            }
            Self::Arpeggio { interval, gate, .. } if gate.0 == 0 || gate > interval => Err(
                Error::Invalid("arpeggio gate must be 1 tick through its interval"),
            ),
            Self::Humanized {
                max_delay,
                velocity_variation,
                ..
            } if max_delay.0 > TICKS_PER_BEAT || velocity_variation > 63 => Err(Error::Invalid(
                "humanization supports at most 1 beat delay and 63 velocity variation",
            )),
            _ => Ok(()),
        }
    }
    pub fn is_repeating(self) -> bool {
        matches!(self, Self::Arpeggio { .. })
    }
    pub fn is_humanized(self) -> bool {
        matches!(self, Self::Humanized { .. })
    }
    pub(crate) fn attack(
        self,
        index: usize,
        count: usize,
        trigger: crate::NoteKey,
        velocity: u8,
    ) -> (BeatTime, u8) {
        if let Self::Humanized {
            max_delay,
            velocity_variation,
            seed,
        } = self
        {
            let source_track = (u64::from(trigger.source.0) << 32) | u64::from(trigger.track.0);
            let seed = crate::random::word(u64::from(seed), trigger.note.0, source_track);
            let delay = crate::random::word(seed, index as u64, 0) % (max_delay.0 + 1);
            let variation = (crate::random::word(seed, index as u64, 1)
                % (2 * u64::from(velocity_variation) + 1)) as i16
                - i16::from(velocity_variation);
            return (
                BeatTime(delay),
                (i16::from(velocity) + variation).clamp(1, 127) as u8,
            );
        }
        (self.delay(index, count), velocity)
    }
    fn delay(self, index: usize, count: usize) -> BeatTime {
        match self {
            Self::Block | Self::Humanized { .. } => BeatTime(0),
            Self::Strum {
                direction,
                interval,
            }
            | Self::Arpeggio {
                direction,
                interval,
                ..
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
    #[serde(default)]
    pub quality: ChordQuality,
    /// Seven input steps per octave around input_anchor. False retains the
    /// original repeating, single-register chord-pad contract.
    #[serde(default)]
    pub input_octaves: bool,
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
    /// A learned entry replaces the diatonic pitches of that degree. Tonic,
    /// scale, extension, suspension and borrowing apply only to empty entries.
    #[serde(default)]
    pub learned: [Option<LearnedChord>; 7],
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
            quality: ChordQuality::Diatonic,
            input_octaves: false,
            suspension: Suspension::None,
            borrowed: false,
            inversion: 0,
            spread: false,
            register: 0,
            voice_leading: false,
            expression: ExpressionPolicy::AllVoices,
            playing: HarmonyPlaying::ChordPads,
            articulation: Articulation::Block,
            learned: [None; 7],
            input_anchor: 30,
            bass_track: None,
        }
    }
}
impl HarmonySettings {
    pub fn requires_chord_grid_schema(self) -> bool {
        self.input_octaves
            || self.quality != ChordQuality::Diatonic
            || !matches!(
                self.extension,
                ChordExtension::Triad | ChordExtension::Seventh | ChordExtension::Ninth
            )
    }
    pub fn has_learned_chords(self) -> bool {
        self.learned.iter().any(Option::is_some)
    }
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
        self.voicing_at_octave(degree, 0, previous)
    }
    /// Explicit octave offset is applied before inversion and voice leading.
    /// Rejects the complete voicing if any pitch falls outside MIDI range.
    pub fn voicing_at_octave(
        self,
        degree: u8,
        octave: i16,
        previous: Option<&Voicing>,
    ) -> Result<Voicing> {
        if !(-10..=10).contains(&octave)
            || degree >= 7
            || self.tonic > 127
            || !(-3..=3).contains(&self.register)
            || !(-4..=4).contains(&self.inversion)
        {
            return Err(Error::Invalid("chord degree or register"));
        }
        let (mut notes, count, mut root) = self.chord_pitches(degree);
        root += octave * 12;
        for note in &mut notes[..count] {
            *note += octave * 12;
        }
        invert(&mut notes[..count], self.inversion);
        if self.spread && count > 1 {
            notes[1] += 12;
            notes[..count].sort_unstable();
        }
        if self.voice_leading
            && let Some(previous) = previous.filter(|p| p.notes().len() == count)
        {
            notes = nearest_voicing(notes, count, previous);
        }
        let mut result = Voicing {
            notes: [0; MAX_VOICING_NOTES],
            count: count as u8,
            root: midi_pitch(root)?,
        };
        for (target, pitch) in result.notes.iter_mut().zip(notes).take(count) {
            *target = midi_pitch(pitch)?;
        }
        Ok(result)
    }
    fn chord_pitches(self, degree: u8) -> ([i16; MAX_VOICING_NOTES], usize, i16) {
        let register = 12 * i16::from(self.register);
        if let Some(chord) = self.learned[usize::from(degree)] {
            let mut notes = [0; MAX_VOICING_NOTES];
            for (target, pitch) in notes.iter_mut().zip(chord.notes()) {
                *target = i16::from(*pitch) + register;
            }
            return (
                notes,
                chord.notes().len(),
                i16::from(chord.root()) + register,
            );
        }
        let scale = if self.borrowed && self.quality == ChordQuality::Diatonic {
            if self.scale == Scale::NaturalMinor {
                Scale::Major
            } else {
                Scale::NaturalMinor
            }
        } else {
            self.scale
        };
        let root = i16::from(self.tonic) + register;
        let degree_root = root + scale.tone(i16::from(degree));
        let mut notes = [0i16; MAX_VOICING_NOTES];
        let third = match self.suspension {
            Suspension::None => 2,
            Suspension::Second => 1,
            Suspension::Fourth => 3,
        };
        for (index, step) in [0, third, 4].into_iter().enumerate() {
            notes[index] = root + scale.tone(i16::from(degree) + step);
        }
        if self.quality != ChordQuality::Diatonic {
            let third = match self.suspension {
                Suspension::Second => 2,
                Suspension::Fourth => 5,
                Suspension::None => {
                    if matches!(self.quality, ChordQuality::Minor | ChordQuality::Diminished) {
                        3
                    } else {
                        4
                    }
                }
            };
            let fifth = match self.quality {
                ChordQuality::Diminished => 6,
                ChordQuality::Augmented => 8,
                _ => 7,
            };
            notes[..3].copy_from_slice(&[degree_root, degree_root + third, degree_root + fifth]);
        }
        let count = self.extend_chord(&mut notes, root, degree_root, scale, degree);
        (notes, count, degree_root)
    }
    fn extend_chord(
        self,
        notes: &mut [i16; MAX_VOICING_NOTES],
        tonic: i16,
        root: i16,
        scale: Scale,
        degree: u8,
    ) -> usize {
        let diatonic = self.quality == ChordQuality::Diatonic;
        let seventh = if diatonic {
            tonic + scale.tone(i16::from(degree) + 6)
        } else {
            root + if self.quality == ChordQuality::Diminished {
                9
            } else {
                10
            }
        };
        let ninth = if diatonic {
            tonic + scale.tone(i16::from(degree) + 8)
        } else {
            root + 14
        };
        match self.extension {
            ChordExtension::Triad => 3,
            ChordExtension::AddNinth => {
                notes[3] = ninth;
                4
            }
            ChordExtension::Sixth => {
                notes[3] = if diatonic {
                    tonic + scale.tone(i16::from(degree) + 5)
                } else {
                    root + 9
                };
                4
            }
            ChordExtension::Seventh => {
                notes[3] = seventh;
                4
            }
            ChordExtension::MajorSeventh => {
                notes[3] = root + 11;
                4
            }
            ChordExtension::MinorSeventh => {
                notes[3] = root + 10;
                4
            }
            ChordExtension::Ninth => {
                notes[3] = seventh;
                notes[4] = ninth;
                5
            }
            ChordExtension::MajorNinth => {
                notes[3] = root + 11;
                notes[4] = root + 14;
                5
            }
        }
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
        if (self.algorithm_version < 6 && self.settings.requires_chord_grid_schema())
            || (self.algorithm_version < 5 && self.settings.articulation.is_humanized())
        {
            return Err(Error::Version(self.algorithm_version));
        }
        let supported = match self.algorithm_version {
            1 => {
                !self.settings.has_learned_chords()
                    && self.settings.articulation == Articulation::Block
            }
            2 => !self.settings.has_learned_chords() && !self.settings.articulation.is_repeating(),
            3 => !self.settings.articulation.is_repeating(),
            4 | 5 => true,
            HARMONY_ALGORITHM_VERSION => true,
            _ => false,
        };
        if !supported {
            return Err(Error::Version(self.algorithm_version));
        }
        self.input.gesture.validate()?;
        self.settings.validate()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Voicing {
    notes: [u8; MAX_VOICING_NOTES],
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
fn nearest_voicing(
    original: [i16; MAX_VOICING_NOTES],
    count: usize,
    previous: &Voicing,
) -> [i16; MAX_VOICING_NOTES] {
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
