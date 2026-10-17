use crate::MidiMessage;
use crest_performance_core::*;

/// Channels are zero-based. One contiguous member zone, main channel excluded.
#[derive(Debug, Clone, Copy)]
pub struct MpeZone {
    pub(crate) main: u8,
    pub(crate) first: u8,
    pub(crate) last: u8,
    bend_semitones: u8,
}
impl MpeZone {
    pub fn new(main: u8, first: u8, last: u8, bend_semitones: u8) -> Result<Self> {
        if main > 15
            || first > last
            || last > 15
            || (first..=last).contains(&main)
            || !(1..=96).contains(&bend_semitones)
        {
            return Err(Error::Invalid("MPE zone"));
        }
        if !((main == 0 && first == 1) || (main == 15 && last == 14)) {
            return Err(Error::Invalid("MPE lower or upper zone required"));
        }
        Ok(Self {
            main,
            first,
            last,
            bend_semitones,
        })
    }
    pub fn lower() -> Self {
        Self {
            main: 0,
            first: 1,
            last: 15,
            bend_semitones: 48,
        }
    }
    pub fn capacity(self) -> usize {
        usize::from(self.last - self.first + 1)
    }
    pub fn bend_semitones(self) -> u8 {
        self.bend_semitones
    }
    pub(crate) fn contains(self, channel: u8) -> bool {
        (self.first..=self.last).contains(&channel)
    }
}
#[derive(Debug, Clone)]
pub struct MidiBatch {
    messages: [MidiMessage; 16],
    len: usize,
}
impl MidiBatch {
    pub(crate) fn new() -> Self {
        Self {
            messages: [MidiMessage::default(); 16],
            len: 0,
        }
    }
    pub(crate) fn push(&mut self, message: MidiMessage) {
        self.messages[self.len] = message;
        self.len += 1;
    }
    pub fn messages(&self) -> &[MidiMessage] {
        &self.messages[..self.len]
    }
}
#[derive(Clone, Copy)]
struct OutputVoice {
    id: VoiceId,
    key: u8,
}
/// Each destination owns its own allocator. Exhaustion rejects a new voice
/// without stealing or changing any existing voice. The host must retain the
/// rejected event or explicitly reject that musical input.
pub struct MpeOutput {
    destination: DestinationId,
    zone: MpeZone,
    voices: [Option<OutputVoice>; 16],
}
impl MpeOutput {
    pub fn new(destination: DestinationId, zone: MpeZone) -> Self {
        Self {
            destination,
            zone,
            voices: [None; 16],
        }
    }
    /// Control-thread setup: announce zone, pitch range on every member, and
    /// null RPN selection. Send before admitting notes to this destination.
    pub fn configuration(&self) -> Vec<MidiMessage> {
        let mut messages = Vec::new();
        append_rpn(&mut messages, 15 - self.zone.main, 6, 0);
        append_rpn(&mut messages, self.zone.main, 6, self.zone.capacity() as u8);
        append_rpn(&mut messages, self.zone.main, 0, 2);
        messages.push(MidiMessage::channel(0xb0, self.zone.main, 64, 0));
        messages.push(MidiMessage::channel(0xe0, self.zone.main, 0, 64));
        for channel in self.zone.first..=self.zone.last {
            append_rpn(&mut messages, channel, 0, self.zone.bend_semitones);
        }
        messages
    }
    pub fn accept(&mut self, event: OutputEvent) -> Result<MidiBatch> {
        if event.destination != self.destination {
            return Err(Error::Invalid("wrong output destination"));
        }
        event.gesture.validate()?;
        let existing = self
            .voices
            .iter()
            .position(|v| v.is_some_and(|v| v.id == event.voice));
        let mut batch = MidiBatch::new();
        match event.gesture {
            Gesture::On {
                key,
                velocity,
                expression,
            } => {
                if existing.is_some() {
                    return Err(Error::DuplicateNote);
                }
                self.check_pitch(expression)?;
                let channel = (self.zone.first..=self.zone.last)
                    .find(|c| self.voices[usize::from(*c)].is_none())
                    .ok_or(Error::Capacity("MPE destination voices"))?;
                batch.push(MidiMessage::channel(0xb0, channel, 64, 0));
                self.expression(&mut batch, channel, expression);
                batch.push(MidiMessage::channel(0x90, channel, key, velocity));
                self.voices[usize::from(channel)] = Some(OutputVoice {
                    id: event.voice,
                    key,
                });
            }
            Gesture::Expression { expression } => {
                self.check_pitch(expression)?;
                self.expression(
                    &mut batch,
                    existing.ok_or(Error::UnknownNote)? as u8,
                    expression,
                );
            }
            Gesture::Off { velocity } => {
                let channel = existing.ok_or(Error::UnknownNote)?;
                let voice = self.voices[channel].take().ok_or(Error::UnknownNote)?;
                batch.push(MidiMessage::channel(
                    0x80,
                    channel as u8,
                    voice.key,
                    velocity,
                ));
                self.expression(&mut batch, channel as u8, Expression::default());
            }
        }
        Ok(batch)
    }
    /// Emergency/control-thread cleanup uses allocated ownership, including
    /// when an earlier musical event was rejected before reaching the sink.
    pub fn release_all(&mut self) -> Vec<MidiMessage> {
        let mut messages = Vec::with_capacity(self.zone.capacity() * 5 + 2);
        for channel in self.zone.first..=self.zone.last {
            if let Some(voice) = self.voices[usize::from(channel)].take() {
                let mut batch = MidiBatch::new();
                batch.push(MidiMessage::channel(0x80, channel, voice.key, 0));
                batch.push(MidiMessage::channel(0xb0, channel, 64, 0));
                self.expression(&mut batch, channel, Expression::default());
                messages.extend_from_slice(batch.messages());
            }
        }
        messages.push(MidiMessage::channel(0xb0, self.zone.main, 64, 0));
        messages.push(MidiMessage::channel(0xe0, self.zone.main, 0, 64));
        messages
    }
    fn check_pitch(&self, expression: Expression) -> Result<()> {
        check_pitch(expression, self.zone.bend_semitones)
    }
    fn expression(&self, batch: &mut MidiBatch, channel: u8, expression: Expression) {
        append_expression(batch, channel, expression, self.zone.bend_semitones);
    }
}
pub(crate) fn check_pitch(expression: Expression, bend_semitones: u8) -> Result<()> {
    let range = i32::from(bend_semitones) * 1_000_000;
    if expression.pitch < -range || expression.pitch >= range {
        return Err(Error::Invalid("expression exceeds destination bend range"));
    }
    Ok(())
}
pub(crate) fn append_expression(
    batch: &mut MidiBatch,
    channel: u8,
    expression: Expression,
    bend_semitones: u8,
) {
    let bend = 8192 + i64::from(expression.pitch) * 8192 / (i64::from(bend_semitones) * 1_000_000);
    batch.push(MidiMessage::channel(
        0xe0,
        channel,
        (bend & 127) as u8,
        (bend >> 7) as u8,
    ));
    batch.push(MidiMessage::channel(
        0xb0,
        channel,
        74,
        unscale7(expression.timbre),
    ));
    batch.push(MidiMessage::channel(
        0xd0,
        channel,
        unscale7(expression.pressure),
        0,
    ));
}

pub(crate) fn append_rpn(messages: &mut Vec<MidiMessage>, channel: u8, parameter: u8, value: u8) {
    for (cc, data) in [
        (101, 0),
        (100, parameter),
        (6, value),
        (38, 0),
        (101, 127),
        (100, 127),
    ] {
        messages.push(MidiMessage::channel(0xb0, channel, cc, data));
    }
}
pub(crate) fn scale7(value: u8) -> u16 {
    (u32::from(value) * 65_535 / 127) as u16
}
pub(crate) fn unscale7(value: u16) -> u8 {
    ((u32::from(value) * 127 + 32_767) / 65_535) as u8
}
