//! Stock User Firmware Mode row X decoding, including firmware slide ownership.
use crate::{Geometry, MidiMessage, user_pad};
use crest_performance_core::{Error, Result};

pub struct UserRowX {
    row: u8,
    geometry: Geometry,
    owner: Option<u8>,
    slide_from: Option<u8>,
    held: [bool; 26],
    lsb: [Option<u8>; 26],
}
impl UserRowX {
    pub fn new(row: u8, geometry: Geometry) -> Result<Self> {
        if row >= 8 {
            return Err(Error::Invalid("user-mode row"));
        }
        Ok(Self {
            row,
            geometry,
            owner: None,
            slide_from: None,
            held: [false; 26],
            lsb: [None; 26],
        })
    }
    pub fn reset(&mut self) {
        self.owner = None;
        self.slide_from = None;
        self.held.fill(false);
        self.lsb.fill(None);
    }
    pub fn configuration(&self, enabled: bool) -> [MidiMessage; 2] {
        [
            MidiMessage::channel(0xb0, self.row, 9, u8::from(enabled)),
            MidiMessage::channel(0xb0, self.row, 10, u8::from(enabled)),
        ]
    }
    /// First new contact owns the row until release. Other held fingers cannot
    /// steal it; the firmware's CC119/NoteOn handover preserves that ownership.
    /// A complete LSB-then-MSB pair is required for every position sample.
    pub fn accept(&mut self, message: MidiMessage) -> Result<Option<u16>> {
        let [status, number, value] = message.bytes();
        if status & 15 != self.row {
            return Ok(None);
        }
        if let Some(pad) = user_pad(message, self.geometry)? {
            if pad.column == 0 {
                return Ok(None);
            }
            self.accept_pad(pad.column, pad.pressed);
            return Ok(None);
        }
        if status & 0xf0 != 0xb0 {
            self.slide_from = None;
            return Ok(None);
        }
        if number == 119 {
            self.slide_from = self.owner.filter(|owner| *owner == value);
            return Ok(None);
        }
        self.slide_from = None;
        let Some(column) = self.owner else {
            return Ok(None);
        };
        if number == column + 32 {
            self.lsb[usize::from(column)] = Some(value);
        }
        if number != column {
            return Ok(None);
        }
        let Some(lsb) = self.lsb[usize::from(column)].take() else {
            return Ok(None);
        };
        let raw = (u32::from(value) << 7) | u32::from(lsb);
        // Pinned upstream documents global edge-to-edge ranges. Clamp sensor
        // overshoot at the endpoints, not per-cell pitch or application colors.
        let maximum = match self.geometry {
            Geometry::Pads200 => 4265,
            Geometry::Pads128 => 2730,
        };
        Ok(Some(
            ((raw.min(maximum) * 65_535 + maximum / 2) / maximum) as u16,
        ))
    }
    fn accept_pad(&mut self, column: u8, pressed: bool) {
        let was_held = self.held[usize::from(column)];
        self.held[usize::from(column)] = pressed;
        self.lsb[usize::from(column)] = None;
        if pressed && !was_held {
            if self.owner.is_none() || self.slide_from == self.owner {
                self.owner = Some(column);
            }
        } else if !pressed && self.owner == Some(column) {
            self.owner = None;
        }
        self.slide_from = None;
    }
}
