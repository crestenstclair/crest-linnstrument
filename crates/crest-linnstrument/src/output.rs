//! Explicit destination profiles. Reduction never masquerades as independent MPE.
use crate::{MidiBatch, MidiMessage, MpeOutput, MpeZone, mpe::*};
use crest_performance_core::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PolyExpression {
    /// Accept per-key pressure; reject pitch/timbre that a shared channel cannot own independently.
    RejectUnsupported,
    /// Explicitly discard pitch/timbre, preserving per-key pressure and note timing.
    PressureOnly,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum OutputProfile {
    Mpe {
        main: u8,
        first: u8,
        last: u8,
        bend_semitones: u8,
    },
    Polyphonic {
        channel: u8,
        voices: u16,
        expression: PolyExpression,
    },
    /// Last-held-note priority. Releasing the leader reattacks the preceding
    /// held note with its latest expression; up to 128 held notes are remembered.
    Monophonic { channel: u8, bend_semitones: u8 },
}
impl Default for OutputProfile {
    fn default() -> Self {
        Self::Mpe {
            main: 0,
            first: 1,
            last: 15,
            bend_semitones: 48,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct OutputCapabilities {
    pub sounding_voices: usize,
    pub held_notes: usize,
    pub independent_pitch: bool,
    pub independent_timbre: bool,
    pub independent_pressure: bool,
    pub pitch_bend_semitones: Option<u8>,
}
impl OutputProfile {
    pub fn capabilities(self) -> Result<OutputCapabilities> {
        match self {
            Self::Mpe {
                main,
                first,
                last,
                bend_semitones,
            } => {
                let zone = MpeZone::new(main, first, last, bend_semitones)?;
                Ok(OutputCapabilities {
                    sounding_voices: zone.capacity(),
                    held_notes: zone.capacity(),
                    independent_pitch: true,
                    independent_timbre: true,
                    independent_pressure: true,
                    pitch_bend_semitones: Some(bend_semitones),
                })
            }
            Self::Polyphonic {
                channel, voices, ..
            } if channel <= 15 && (1..=128).contains(&voices) => Ok(OutputCapabilities {
                sounding_voices: usize::from(voices),
                held_notes: usize::from(voices),
                independent_pitch: false,
                independent_timbre: false,
                independent_pressure: true,
                pitch_bend_semitones: None,
            }),
            Self::Monophonic {
                channel,
                bend_semitones,
            } if channel <= 15 && (1..=96).contains(&bend_semitones) => Ok(OutputCapabilities {
                sounding_voices: 1,
                held_notes: 128,
                independent_pitch: false,
                independent_timbre: false,
                independent_pressure: false,
                pitch_bend_semitones: Some(bend_semitones),
            }),
            _ => Err(Error::Invalid("output profile")),
        }
    }
    /// A host requiring independent expressive polyphony must negotiate it
    /// before starting. Choosing a reduction profile is a separate explicit act.
    pub fn require_mpe(self, voices: usize, bend_semitones: u8) -> Result<()> {
        let capabilities = self.capabilities()?;
        if voices == 0
            || !capabilities.independent_pitch
            || !capabilities.independent_timbre
            || !capabilities.independent_pressure
            || capabilities.sounding_voices < voices
            || capabilities.pitch_bend_semitones != Some(bend_semitones)
        {
            return Err(Error::Invalid(
                "destination cannot satisfy requested MPE profile",
            ));
        }
        Ok(())
    }
}
pub struct MidiOutput {
    profile: OutputProfile,
    backend: Backend,
}
enum Backend {
    Mpe(Box<MpeOutput>),
    Channel(ChannelOutput),
}
impl MidiOutput {
    pub fn new(destination: DestinationId, profile: OutputProfile) -> Result<Self> {
        let capacity = profile.capabilities()?.held_notes;
        let backend = match profile {
            OutputProfile::Mpe {
                main,
                first,
                last,
                bend_semitones,
            } => Backend::Mpe(Box::new(MpeOutput::new(
                destination,
                MpeZone::new(main, first, last, bend_semitones)?,
            ))),
            OutputProfile::Polyphonic { channel, .. }
            | OutputProfile::Monophonic { channel, .. } => Backend::Channel(ChannelOutput {
                destination,
                channel,
                profile,
                voices: Vec::with_capacity(capacity),
                capacity,
            }),
        };
        Ok(Self { profile, backend })
    }
    pub fn profile(&self) -> OutputProfile {
        self.profile
    }
    pub fn configuration(&self) -> Vec<MidiMessage> {
        match &self.backend {
            Backend::Mpe(o) => o.configuration(),
            Backend::Channel(o) => o.configuration(),
        }
    }
    pub fn accept(&mut self, event: OutputEvent) -> Result<MidiBatch> {
        match &mut self.backend {
            Backend::Mpe(o) => o.accept(event),
            Backend::Channel(o) => o.accept(event),
        }
    }
    pub fn release_all(&mut self) -> Vec<MidiMessage> {
        match &mut self.backend {
            Backend::Mpe(o) => o.release_all(),
            Backend::Channel(o) => o.release_all(),
        }
    }
}
#[derive(Clone, Copy)]
struct ChannelVoice {
    id: VoiceId,
    key: u8,
    velocity: u8,
    expression: Expression,
}
struct ChannelOutput {
    destination: DestinationId,
    channel: u8,
    profile: OutputProfile,
    voices: Vec<ChannelVoice>,
    capacity: usize,
}
impl ChannelOutput {
    fn mono_range(&self) -> Option<u8> {
        if let OutputProfile::Monophonic { bend_semitones, .. } = self.profile {
            Some(bend_semitones)
        } else {
            None
        }
    }
    fn configuration(&self) -> Vec<MidiMessage> {
        let mut messages = Vec::new();
        append_rpn(&mut messages, 0, 6, 0);
        append_rpn(&mut messages, 15, 6, 0);
        if let Some(range) = self.mono_range() {
            append_rpn(&mut messages, self.channel, 0, range);
        }
        messages.extend([
            MidiMessage::channel(0xb0, self.channel, 64, 0),
            MidiMessage::channel(0xe0, self.channel, 0, 64),
            MidiMessage::channel(0xb0, self.channel, 74, 64),
            MidiMessage::channel(0xd0, self.channel, 0, 0),
        ]);
        messages
    }
    fn check_expression(&self, expression: Expression) -> Result<()> {
        if let Some(range) = self.mono_range() {
            return check_pitch(expression, range);
        }
        if matches!(
            self.profile,
            OutputProfile::Polyphonic {
                expression: PolyExpression::RejectUnsupported,
                ..
            }
        ) && (expression.pitch != 0 || unscale7(expression.timbre) != 64)
        {
            return Err(Error::Invalid(
                "polyphonic destination cannot preserve pitch/timbre",
            ));
        }
        Ok(())
    }
    fn accept(&mut self, event: OutputEvent) -> Result<MidiBatch> {
        if event.destination != self.destination {
            return Err(Error::Invalid("wrong output destination"));
        }
        event.gesture.validate()?;
        let index = self.voices.iter().position(|v| v.id == event.voice);
        let mut batch = MidiBatch::new();
        match event.gesture {
            Gesture::On {
                key,
                velocity,
                expression,
            } => {
                if index.is_some() {
                    return Err(Error::DuplicateNote);
                }
                if self.voices.len() == self.capacity {
                    return Err(Error::Capacity("destination held notes"));
                }
                self.check_expression(expression)?;
                if self.mono_range().is_none() && self.voices.iter().any(|v| v.key == key) {
                    return Err(Error::State(
                        "ordinary MIDI cannot own overlapping identical pitches",
                    ));
                }
                if self.mono_range().is_some()
                    && let Some(voice) = self.voices.last()
                {
                    self.off(&mut batch, *voice, 0);
                }
                let voice = ChannelVoice {
                    id: event.voice,
                    key,
                    velocity,
                    expression,
                };
                self.on(&mut batch, voice);
                self.voices.push(voice);
            }
            Gesture::Expression { expression } => {
                let index = index.ok_or(Error::UnknownNote)?;
                self.check_expression(expression)?;
                self.voices[index].expression = expression;
                if self.mono_range().is_none() || index + 1 == self.voices.len() {
                    self.expression(&mut batch, self.voices[index]);
                }
            }
            Gesture::Off { velocity } => {
                let index = index.ok_or(Error::UnknownNote)?;
                let audible = self.mono_range().is_none() || index + 1 == self.voices.len();
                let voice = self.voices.remove(index);
                if audible {
                    self.off(&mut batch, voice, velocity);
                    if self.mono_range().is_some()
                        && let Some(previous) = self.voices.last()
                    {
                        self.on(&mut batch, *previous);
                    }
                }
            }
        }
        Ok(batch)
    }
    fn expression(&self, batch: &mut MidiBatch, voice: ChannelVoice) {
        if let Some(range) = self.mono_range() {
            append_expression(batch, self.channel, voice.expression, range);
        } else {
            batch.push(MidiMessage::channel(
                0xa0,
                self.channel,
                voice.key,
                unscale7(voice.expression.pressure),
            ));
        }
    }
    fn on(&self, batch: &mut MidiBatch, voice: ChannelVoice) {
        if self.mono_range().is_some() {
            batch.push(MidiMessage::channel(0xb0, self.channel, 64, 0));
            self.expression(batch, voice);
        }
        batch.push(MidiMessage::channel(
            0x90,
            self.channel,
            voice.key,
            voice.velocity,
        ));
        // Poly key pressure addresses an active note; send its initial value
        // after Note On. Member/channel expression is established beforehand.
        if self.mono_range().is_none() {
            self.expression(batch, voice);
        }
    }
    fn off(&self, batch: &mut MidiBatch, voice: ChannelVoice, velocity: u8) {
        batch.push(MidiMessage::channel(
            0x80,
            self.channel,
            voice.key,
            velocity,
        ));
        self.expression(
            batch,
            ChannelVoice {
                expression: Expression::default(),
                ..voice
            },
        );
    }
    fn release_all(&mut self) -> Vec<MidiMessage> {
        let mut messages = Vec::new();
        if self.mono_range().is_some() {
            if let Some(voice) = self.voices.last() {
                let mut batch = MidiBatch::new();
                self.off(&mut batch, *voice, 0);
                messages.extend_from_slice(batch.messages());
            }
        } else {
            for voice in &self.voices {
                let mut batch = MidiBatch::new();
                self.off(&mut batch, *voice, 0);
                messages.extend_from_slice(batch.messages());
            }
        }
        self.voices.clear();
        messages.push(MidiMessage::channel(0xb0, self.channel, 64, 0));
        messages
    }
}
