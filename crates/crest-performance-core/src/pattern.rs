//! Seeded, bounded accompaniment generation. This is control-thread work.
//! Rendered clips replay exactly; harmonic following is an explicit opt-in.
use crate::random::word as random;
use crate::*;
use serde::{Deserialize, Serialize};

pub const PATTERN_ALGORITHM_VERSION: u32 = 4;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternSettings {
    pub steps: u16,
    pub pulses: u16,
    pub rotation: u16,
    pub step: BeatTime,
    pub gate: BeatTime,
    /// Independent pitch-lane cycle, measured in rhythmic steps.
    pub pitch_steps: u16,
    /// Inclusive chord-tone indices; negative values descend into lower octaves.
    pub tone_min: i16,
    pub tone_max: i16,
    pub seed: u64,
    pub velocity: u8,
    pub velocity_variation: u8,
    /// Nonnegative attack jitter, bounded so notes stay within their own step.
    pub timing_variation: BeatTime,
    pub follow_harmony: bool,
}
impl Default for PatternSettings {
    fn default() -> Self {
        Self {
            steps: 16,
            pulses: 5,
            rotation: 0,
            step: BeatTime(TICKS_PER_BEAT / 4),
            gate: BeatTime(TICKS_PER_BEAT / 8),
            pitch_steps: 8,
            tone_min: 0,
            tone_max: 5,
            seed: 1,
            velocity: 90,
            velocity_variation: 0,
            timing_variation: BeatTime(0),
            follow_harmony: false,
        }
    }
}
impl PatternSettings {
    pub fn validate(self) -> Result<()> {
        if !(1..=128).contains(&self.steps)
            || self.pulses > self.steps
            || self.rotation >= self.steps
            || !(1..=128).contains(&self.pitch_steps)
            || self.tone_min < -16
            || self.tone_max > 16
            || self.tone_min > self.tone_max
            || self.step.0 == 0
            || self.gate.0 == 0
            || self.gate.checked_add(self.timing_variation)? > self.step
            || !(1..=127).contains(&self.velocity)
            || self.velocity_variation > 63
        {
            return Err(Error::Invalid("pattern bounds"));
        }
        self.step
            .0
            .checked_mul(u64::from(self.steps))
            .ok_or(Error::Invalid("pattern length overflow"))?;
        Ok(())
    }
    /// Euclidean distribution with the first pulse at step zero before rotation.
    pub fn has_pulse(self, step: u16) -> Result<bool> {
        self.validate()?;
        if step >= self.steps {
            return Err(Error::Invalid("pattern step"));
        }
        let shifted = (u32::from(step) + u32::from(self.steps) - u32::from(self.rotation))
            % u32::from(self.steps);
        Ok((shifted * u32::from(self.pulses)) % u32::from(self.steps) < u32::from(self.pulses))
    }
    pub fn render(self, harmony: HarmonySettings, track: TrackId) -> Result<Clip> {
        self.validate()?;
        harmony.validate()?;
        let voicing = harmony.voicing(harmony.degree, None)?;
        // Validate the entire pitch range, not only this seed's selected notes.
        for tone in self.tone_min..=self.tone_max {
            voicing.strumplate_pitch(tone)?;
        }
        let mut notes = Vec::with_capacity(usize::from(self.pulses));
        for step in 0..self.steps {
            if self.has_pulse(step)? {
                notes.push(self.render_note(step, &voicing, track)?);
            }
        }
        let mut clip = Clip::from_notes(BeatTime(self.step.0 * u64::from(self.steps)), &notes)?;
        clip.pattern_origin = vec![PatternOrigin {
            algorithm_version: PATTERN_ALGORITHM_VERSION,
            settings: self,
            harmony,
        }];
        if self.follow_harmony {
            clip.follow = Some(HarmonyFollow { reference: harmony });
        }
        clip.validate(track)?;
        Ok(clip)
    }
    fn render_note(self, step: u16, voicing: &Voicing, track: TrackId) -> Result<Note> {
        let range = (self.tone_max - self.tone_min + 1) as u64;
        let tone = self.tone_min
            + (random(self.seed, u64::from(step % self.pitch_steps), 0) % range) as i16;
        let jitter = random(self.seed, u64::from(step), 1) % (self.timing_variation.0 + 1);
        let variation = (random(self.seed, u64::from(step), 2)
            % (2 * u64::from(self.velocity_variation) + 1)) as i16
            - i16::from(self.velocity_variation);
        Ok(Note {
            id: NoteKey {
                source: GENERATED_SOURCE,
                track,
                note: NoteId(u64::from(step) + 1),
            },
            key: voicing.strumplate_pitch(tone)?,
            velocity: (i16::from(self.velocity) + variation).clamp(1, 127) as u8,
            release_velocity: 0,
            start: BeatTime(u64::from(step) * self.step.0 + jitter),
            duration: self.gate,
            expression: ExpressionCurve {
                points: vec![ExpressionPoint {
                    offset: BeatTime(0),
                    value: Expression::default(),
                }],
            },
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatternOrigin {
    pub algorithm_version: u32,
    pub settings: PatternSettings,
    pub harmony: HarmonySettings,
}
impl PatternOrigin {
    pub fn validate(self) -> Result<()> {
        if self.algorithm_version < PATTERN_ALGORITHM_VERSION
            && self.harmony.articulation.is_humanized()
        {
            return Err(Error::Version(self.algorithm_version));
        }
        let supported = match self.algorithm_version {
            1 => !self.harmony.has_learned_chords() && !self.harmony.articulation.is_repeating(),
            2 => !self.harmony.articulation.is_repeating(),
            3 => true,
            PATTERN_ALGORITHM_VERSION => true,
            _ => false,
        };
        if !supported {
            return Err(Error::Version(self.algorithm_version));
        }
        self.settings.validate()?;
        self.harmony.validate()
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarmonyFollow {
    pub reference: HarmonySettings,
}
impl HarmonyFollow {
    /// Map the nearest reference chord tone to the current chord, retaining
    /// the original chromatic offset. Ties choose the lower tone. Evaluate at
    /// each attack only; existing notes and their expression retain ownership.
    pub fn pitch(self, key: u8, current: HarmonySettings) -> Result<u8> {
        if key > 127 {
            return Err(Error::Invalid("follower pitch"));
        }
        self.reference.validate()?;
        current.validate()?;
        let reference = self.reference.voicing(self.reference.degree, None)?;
        let target = current.voicing(current.degree, None)?;
        let (tone, offset) = nearest_tone(key, &reference);
        let pitch = i16::from(target.strumplate_pitch(tone)?) + offset;
        if !(0..=127).contains(&pitch) {
            return Err(Error::Invalid("follower outside MIDI range"));
        }
        Ok(pitch as u8)
    }
}
fn nearest_tone(key: u8, reference: &Voicing) -> (i16, i16) {
    let mut best = (0, i16::MAX);
    let mut best_pitch = u8::MAX;
    // Covers every MIDI octave even for a five-note voicing at either edge.
    for tone in -60..=60 {
        if let Ok(pitch) = reference.strumplate_pitch(tone) {
            let distance = i16::from(key) - i16::from(pitch);
            if (distance.abs(), pitch) < (best.1.abs(), best_pitch) {
                best = (tone, distance);
                best_pitch = pitch;
            }
        }
    }
    best
}
