use crate::{MidiMessage, MpeZone, mpe::scale7};
use crest_performance_core::*;

const MAX_INPUT_VOICES: usize = 128;
#[derive(Clone, Copy)]
enum Profile {
    Mpe(MpeZone),
    Polyphonic { channel: Option<u8> },
}
#[derive(Clone, Copy)]
struct InputVoice {
    note: NoteKey,
    channel: u8,
    key: u8,
    released: Option<u8>,
    poly_pressure: Option<u16>,
}

/// MIDI input ownership, independent of output channel allocation. Sustain is
/// rendered as note duration. Released sustained notes retain their last
/// expression, so reusing an input channel cannot bend an old sustained voice.
pub struct NoteInput {
    profile: Profile,
    source: SourceId,
    track: TrackId,
    next: u64,
    voices: Vec<InputVoice>,
    events: Vec<InputEvent>,
    expressions: [Expression; 16],
    bend_ranges: [u32; 16],
    rpn: [[u8; 2]; 16],
    sustain: [bool; 16],
    main_pitch: i32,
    bends: [i16; 16],
}
impl NoteInput {
    pub fn mpe(zone: MpeZone, source: SourceId, track: TrackId) -> Self {
        let mut input = Self::new(Profile::Mpe(zone), source, track, zone.bend_semitones());
        input.bend_ranges[usize::from(zone.main)] = 2_000_000;
        input
    }
    pub fn polyphonic(
        channel: Option<u8>,
        bend_semitones: u8,
        source: SourceId,
        track: TrackId,
    ) -> Result<Self> {
        if channel.is_some_and(|c| c > 15) || !(1..=96).contains(&bend_semitones) {
            return Err(Error::Invalid("polyphonic MIDI profile"));
        }
        Ok(Self::new(
            Profile::Polyphonic { channel },
            source,
            track,
            bend_semitones,
        ))
    }
    fn new(profile: Profile, source: SourceId, track: TrackId, bend: u8) -> Self {
        Self {
            profile,
            source,
            track,
            next: 1,
            voices: Vec::with_capacity(MAX_INPUT_VOICES),
            events: Vec::with_capacity(MAX_INPUT_VOICES),
            expressions: [Expression::default(); 16],
            bend_ranges: [u32::from(bend) * 1_000_000; 16],
            rpn: [[127; 2]; 16],
            sustain: [false; 16],
            main_pitch: 0,
            bends: [0; 16],
        }
    }
    /// Reconnect clears ownership but does not reuse note identities or forget
    /// negotiated bend ranges. A new source/profile constructs a new adapter.
    pub fn reset(&mut self) {
        self.voices.clear();
        self.events.clear();
        self.expressions.fill(Expression::default());
        self.rpn.fill([127; 2]);
        self.sustain.fill(false);
        self.main_pitch = 0;
        self.bends.fill(0);
    }
    pub fn active_notes(&self) -> usize {
        self.voices.len()
    }
    pub fn bend_range_millionths(&self, channel: u8) -> Result<u32> {
        self.bend_ranges
            .get(usize::from(channel))
            .copied()
            .ok_or(Error::Invalid("MIDI channel"))
    }
    /// Batch storage is reused on the next call. Channel-wide expression and
    /// pedal release can produce several events, in original attack order.
    pub fn accept(&mut self, at: BeatTime, message: MidiMessage) -> Result<&[InputEvent]> {
        self.events.clear();
        let [status, a, b] = message.bytes();
        let channel = status & 15;
        if status >= 0xf0 || !self.accepts(channel) {
            return Ok(&self.events);
        }
        if a > 127 || b > 127 {
            return Err(Error::Invalid("MIDI data"));
        }
        let index = usize::from(channel);
        match status & 0xf0 {
            0x90 if b > 0 && !self.is_main(channel) => self.note_on(at, channel, a, b)?,
            0x80 | 0x90 if !self.is_main(channel) => self.note_off(at, channel, a, b)?,
            0xe0 => {
                let bend = i32::from(a) + (i32::from(b) << 7) - 8192;
                let pitch = (i64::from(bend) * i64::from(self.bend_ranges[index]) / 8192) as i32;
                let previous = (self.main_pitch, self.expressions[index].pitch);
                if self.is_main(channel) {
                    self.main_pitch = pitch;
                } else {
                    self.expressions[index].pitch = pitch;
                }
                if let Err(error) = self.emit_expression(at, channel, None) {
                    self.main_pitch = previous.0;
                    self.expressions[index].pitch = previous.1;
                    return Err(error);
                }
                self.bends[index] = bend as i16;
            }
            0xb0 => self.control(at, channel, a, b)?,
            0xd0 if !self.is_main(channel) => {
                self.expressions[index].pressure = scale7(a);
                for voice in &mut self.voices {
                    if voice.channel == channel {
                        voice.poly_pressure = None;
                    }
                }
                self.emit_expression(at, channel, None)?;
            }
            0xa0 if !self.is_main(channel) => {
                // Polyphonic key pressure affects only matching held pitches.
                for voice in &mut self.voices {
                    if voice.channel == channel && voice.key == a && voice.released.is_none() {
                        voice.poly_pressure = Some(scale7(b));
                    }
                }
                self.emit_expression(at, channel, Some(a))?;
            }
            _ => {}
        }
        Ok(&self.events)
    }
    fn accepts(&self, channel: u8) -> bool {
        match self.profile {
            Profile::Mpe(zone) => zone.contains(channel) || zone.main == channel,
            Profile::Polyphonic { channel: selected } => selected.is_none_or(|c| c == channel),
        }
    }
    fn is_main(&self, channel: u8) -> bool {
        matches!(self.profile, Profile::Mpe(zone) if zone.main == channel)
    }
    fn applies(&self, controller: u8, member: u8) -> bool {
        self.is_main(controller) || controller == member
    }
    fn pedal(&self, channel: u8) -> bool {
        self.sustain[usize::from(channel)]
            || match self.profile {
                Profile::Mpe(zone) => self.sustain[usize::from(zone.main)],
                _ => false,
            }
    }
    fn expression(&self, channel: u8) -> Result<Expression> {
        let mut expression = self.expressions[usize::from(channel)];
        expression.pitch += self.main_pitch;
        expression.validate()?;
        Ok(expression)
    }
    fn emit_expression(&mut self, at: BeatTime, channel: u8, key: Option<u8>) -> Result<()> {
        // Validate the whole fan-out before committing events.
        for voice in &self.voices {
            if voice.released.is_none() && self.applies(channel, voice.channel) {
                self.expression(voice.channel)?;
            }
        }
        for voice in &self.voices {
            if voice.released.is_none()
                && self.applies(channel, voice.channel)
                && key.is_none_or(|key| key == voice.key)
            {
                let mut expression = self.expression(voice.channel)?;
                if let Some(pressure) = voice.poly_pressure {
                    expression.pressure = pressure;
                }
                self.events.push(InputEvent {
                    at,
                    note: voice.note,
                    gesture: Gesture::Expression { expression },
                });
            }
        }
        Ok(())
    }
    fn note_on(&mut self, at: BeatTime, channel: u8, key: u8, velocity: u8) -> Result<()> {
        if matches!(self.profile, Profile::Mpe(_))
            && self
                .voices
                .iter()
                .any(|v| v.channel == channel && v.released.is_none())
        {
            return Err(Error::State("MPE member channel reused before release"));
        }
        if self.voices.len() == MAX_INPUT_VOICES {
            return Err(Error::Capacity("MIDI input voices including sustain"));
        }
        let next = self
            .next
            .checked_add(1)
            .ok_or(Error::Capacity("input note identities"))?;
        let expression = self.expression(channel)?;
        let note = NoteKey {
            source: self.source,
            track: self.track,
            note: NoteId(self.next),
        };
        self.next = next;
        self.voices.push(InputVoice {
            note,
            channel,
            key,
            released: None,
            poly_pressure: None,
        });
        self.events.push(InputEvent {
            at,
            note,
            gesture: Gesture::On {
                key,
                velocity,
                expression,
            },
        });
        Ok(())
    }
    fn note_off(&mut self, at: BeatTime, channel: u8, key: u8, velocity: u8) -> Result<()> {
        // MIDI 1.0 has no identity for repeated same-channel pitches: FIFO is explicit.
        let found = self
            .voices
            .iter()
            .position(|v| v.channel == channel && v.key == key && v.released.is_none());
        let Some(index) = found else {
            if matches!(self.profile, Profile::Mpe(_))
                && self
                    .voices
                    .iter()
                    .any(|v| v.channel == channel && v.released.is_none())
            {
                return Err(Error::Invalid("MPE release pitch does not own channel"));
            }
            return Ok(());
        };
        if self.pedal(channel) {
            self.voices[index].released = Some(velocity);
        } else {
            let voice = self.voices.remove(index);
            self.events.push(InputEvent {
                at,
                note: voice.note,
                gesture: Gesture::Off { velocity },
            });
        }
        Ok(())
    }
    fn release_deferred(&mut self, at: BeatTime) {
        let mut index = 0;
        while index < self.voices.len() {
            let voice = self.voices[index];
            if let Some(velocity) = voice.released
                && !self.pedal(voice.channel)
            {
                self.voices.remove(index);
                self.events.push(InputEvent {
                    at,
                    note: voice.note,
                    gesture: Gesture::Off { velocity },
                });
            } else {
                index += 1;
            }
        }
    }
    fn control(&mut self, at: BeatTime, channel: u8, cc: u8, value: u8) -> Result<()> {
        let index = usize::from(channel);
        match cc {
            101 => self.rpn[index][0] = value,
            100 => self.rpn[index][1] = value,
            98 | 99 => self.rpn[index] = [127; 2],
            6 | 38 => self.data_entry(channel, cc, value)?,
            64 => {
                self.sustain[index] = value >= 64;
                self.release_deferred(at);
            }
            74 if !self.is_main(channel) => {
                self.expressions[index].timbre = scale7(value);
                self.emit_expression(at, channel, None)?;
            }
            120 | 123 => {
                let mut index = 0;
                while index < self.voices.len() {
                    let voice = self.voices[index];
                    if !self.applies(channel, voice.channel) {
                        index += 1;
                        continue;
                    }
                    if cc == 123 && self.pedal(voice.channel) {
                        self.voices[index].released = Some(0);
                        index += 1;
                    } else {
                        self.voices.remove(index);
                        self.events.push(InputEvent {
                            at,
                            note: voice.note,
                            gesture: Gesture::Off { velocity: 0 },
                        });
                    }
                }
            }
            121 => {
                self.sustain[index] = false;
                self.bends[index] = 0;
                for voice in &mut self.voices {
                    if voice.channel == channel {
                        voice.poly_pressure = None;
                    }
                }
                if self.is_main(channel) {
                    self.main_pitch = 0;
                } else {
                    self.expressions[index] = Expression::default();
                }
                self.release_deferred(at);
                self.emit_expression(at, channel, None)?;
            }
            _ => {}
        }
        Ok(())
    }
    fn data_entry(&mut self, channel: u8, cc: u8, value: u8) -> Result<()> {
        let index = usize::from(channel);
        match self.rpn[index] {
            [0, 0] => {
                let range = if cc == 6 {
                    u32::from(value) * 1_000_000 + self.bend_ranges[index] % 1_000_000
                } else {
                    if value > 99 {
                        return Err(Error::Invalid("pitch bend cents"));
                    }
                    self.bend_ranges[index] / 1_000_000 * 1_000_000 + u32::from(value) * 10_000
                };
                if range == 0 || range > 96_000_000 {
                    return Err(Error::Invalid("pitch bend range"));
                }
                if range != self.bend_ranges[index]
                    && self.voices.iter().any(|v| self.applies(channel, v.channel))
                {
                    return Err(Error::State(
                        "release notes before changing pitch bend range",
                    ));
                }
                self.bend_ranges[index] = range;
                let pitch = (i64::from(self.bends[index]) * i64::from(range) / 8192) as i32;
                if self.is_main(channel) {
                    self.main_pitch = pitch;
                } else {
                    self.expressions[index].pitch = pitch;
                }
            }
            [0, 6] if cc == 6 && self.is_main(channel) => {
                if !(1..=15).contains(&value) {
                    return Err(Error::Invalid(
                        "MPE member count; disabled zone requires profile change",
                    ));
                }
                let Profile::Mpe(zone) = self.profile else {
                    unreachable!()
                };
                if value as usize != zone.capacity() && !self.voices.is_empty() {
                    return Err(Error::State("release notes before changing MPE zone"));
                }
                self.profile = Profile::Mpe(if channel == 0 {
                    MpeZone::new(0, 1, value, zone.bend_semitones())?
                } else {
                    MpeZone::new(15, 15 - value, 14, zone.bend_semitones())?
                });
            }
            _ => {}
        }
        Ok(())
    }
}
