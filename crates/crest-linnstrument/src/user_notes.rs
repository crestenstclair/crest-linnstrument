//! Per-cell expressive ownership for stock User Firmware Mode. No synthetic MIDI:
//! input is the firmware's coordinate Note On/Off, X/Y CC and poly pressure stream.
use crate::{Geometry, MidiMessage, mpe::scale7};
use crest_performance_core::*;

#[derive(Clone, Copy)]
struct CellVoice {
    note: NoteKey,
    expression: Expression,
    origin_x: Option<u16>,
}

/// Chord surfaces use independent cells, with slide mode disabled. Crossing a
/// cell boundary releases/retriggers; in-cell X bends one semitone per cell width.
/// Stock Play MPE and its continuous-slide interpretation remain separate.
pub struct UserNoteInput {
    geometry: Geometry,
    source: SourceId,
    track: TrackId,
    next: u64,
    voices: [[Option<CellVoice>; 8]; 26],
    x_lsb: [[Option<u8>; 8]; 26],
}
impl UserNoteInput {
    pub fn new(geometry: Geometry, source: SourceId, track: TrackId) -> Self {
        Self {
            geometry,
            source,
            track,
            next: 1,
            voices: [[None; 8]; 26],
            x_lsb: [[None; 8]; 26],
        }
    }
    /// Clear ownership after the engine has released notes; never reuse IDs.
    pub fn reset(&mut self) {
        self.voices.fill([None; 8]);
        self.x_lsb.fill([None; 8]);
    }
    pub fn configuration(&self) -> Vec<MidiMessage> {
        (0..8).flat_map(row_configuration).collect()
    }
    pub fn held(&self, column: u8, row: u8) -> bool {
        column < self.geometry.columns()
            && row < 8
            && self.voices[usize::from(column)][usize::from(row)].is_some()
    }
    /// `attack_key` is the host layout's musical mapping, or None for an inert
    /// or control cell. Releases/expression always follow their original owner.
    pub fn accept(
        &mut self,
        at: BeatTime,
        message: MidiMessage,
        attack_key: Option<u8>,
    ) -> Result<Option<InputEvent>> {
        let [status, number, value] = message.bytes();
        let row = usize::from(status & 15);
        if status >= 0xf0 || row >= 8 {
            return Ok(None);
        }
        if number > 127 || value > 127 {
            return Err(Error::Invalid("user-mode MIDI data"));
        }
        let Some(column) = cell_column(status, number, self.geometry) else {
            return Ok(None);
        };
        let column = usize::from(column);
        if status & 0xf0 == 0x90 && value > 0 {
            return self.attack(at, column, row, value, attack_key);
        }
        let voice = &mut self.voices[column][row];
        if matches!(status & 0xf0, 0x80 | 0x90) {
            self.x_lsb[column][row] = None;
            return Ok(voice.take().map(|v| InputEvent {
                at,
                note: v.note,
                gesture: Gesture::Off { velocity: 0 },
            }));
        }
        self.expression(at, column, row, status, number, value)
    }
    fn attack(
        &mut self,
        at: BeatTime,
        column: usize,
        row: usize,
        velocity: u8,
        key: Option<u8>,
    ) -> Result<Option<InputEvent>> {
        if self.voices[column][row].is_some() {
            return Ok(None);
        }
        let Some(key) = key else {
            return Ok(None);
        };
        if key > 127 {
            return Err(Error::Invalid("user-mode input key"));
        }
        let next = self
            .next
            .checked_add(1)
            .ok_or(Error::Capacity("user-mode note identities"))?;
        let note = NoteKey {
            source: self.source,
            track: self.track,
            note: NoteId(self.next),
        };
        self.next = next;
        self.voices[column][row] = Some(CellVoice {
            note,
            expression: Expression::default(),
            origin_x: None,
        });
        self.x_lsb[column][row] = None;
        Ok(Some(InputEvent {
            at,
            note,
            gesture: Gesture::On {
                key,
                velocity,
                expression: Expression::default(),
            },
        }))
    }
    fn expression(
        &mut self,
        at: BeatTime,
        column: usize,
        row: usize,
        status: u8,
        number: u8,
        value: u8,
    ) -> Result<Option<InputEvent>> {
        let voice = &mut self.voices[column][row];
        let Some(voice) = voice else {
            return Ok(None);
        };
        let previous = voice.expression;
        match status & 0xf0 {
            0xa0 => voice.expression.pressure = scale7(value),
            0xb0 if number >= 64 => voice.expression.timbre = scale7(value),
            0xb0 if number >= 32 => {
                self.x_lsb[column][row] = Some(value);
                return Ok(None);
            }
            0xb0 => {
                let Some(lsb) = self.x_lsb[column][row].take() else {
                    return Ok(None);
                };
                let x = (u16::from(value) << 7) | u16::from(lsb);
                let origin = *voice.origin_x.get_or_insert(x);
                // Upstream X range is 4265/25 or 2730/16 per playing cell.
                let span = match self.geometry {
                    Geometry::Pads200 => 4265,
                    Geometry::Pads128 => 2730,
                };
                voice.expression.pitch = (i64::from(i32::from(x) - i32::from(origin))
                    * i64::from(self.geometry.columns() - 1)
                    * 1_000_000
                    / span)
                    .clamp(-96_000_000, 96_000_000) as i32;
            }
            _ => return Ok(None),
        }
        Ok((voice.expression != previous).then_some(InputEvent {
            at,
            note: voice.note,
            gesture: Gesture::Expression {
                expression: voice.expression,
            },
        }))
    }
    /// Reject an attack that the engine could not allocate, so later expression
    /// and release packets cannot address an unaccepted note.
    pub fn reject(&mut self, column: u8, row: u8) {
        if column < self.geometry.columns() && row < 8 {
            self.voices[usize::from(column)][usize::from(row)] = None;
            self.x_lsb[usize::from(column)][usize::from(row)] = None;
        }
    }
}

fn cell_column(status: u8, number: u8, geometry: Geometry) -> Option<u8> {
    let column = match status & 0xf0 {
        0x80 | 0x90 | 0xa0 => number,
        0xb0 if number < 26 => number,
        0xb0 if (32..58).contains(&number) => number - 32,
        0xb0 if (64..90).contains(&number) => number - 64,
        _ => return None,
    };
    (column > 0 && column < geometry.columns()).then_some(column)
}

fn row_configuration(row: u8) -> [MidiMessage; 4] {
    [9, 10, 11, 12].map(|cc| MidiMessage::channel(0xb0, row, cc, u8::from(cc != 9)))
}
