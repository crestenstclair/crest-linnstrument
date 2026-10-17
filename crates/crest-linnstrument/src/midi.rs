use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "WireMessage")]
pub struct MidiMessage {
    bytes: [u8; 3],
    len: u8,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WireMessage {
    bytes: [u8; 3],
    len: u8,
}
impl TryFrom<WireMessage> for MidiMessage {
    type Error = &'static str;
    fn try_from(wire: WireMessage) -> Result<Self, Self::Error> {
        let expected = match wire.bytes[0] {
            0x80..=0xbf | 0xe0..=0xef | 0xf2 => 3,
            0xc0..=0xdf | 0xf1 | 0xf3 => 2,
            0xf6 | 0xf8..=0xff => 1,
            _ => return Err("invalid MIDI status"),
        };
        if wire.len != expected
            || wire.bytes[1..usize::from(wire.len)]
                .iter()
                .any(|b| *b >= 128)
        {
            return Err("invalid MIDI length or data");
        }
        Ok(Self {
            bytes: wire.bytes,
            len: wire.len,
        })
    }
}
impl MidiMessage {
    pub fn bytes(&self) -> [u8; 3] {
        self.bytes
    }

    pub fn channel(status: u8, channel: u8, first: u8, second: u8) -> Self {
        Self {
            bytes: [status | channel, first, second],
            len: if status == 0xc0 || status == 0xd0 {
                2
            } else {
                3
            },
        }
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

/// Streaming MIDI 1.0 parser. Fixed storage; SysEx is deliberately skipped,
/// including arbitrarily long SysEx. Real-time bytes never disturb running
/// status or partial messages. System common messages clear running status.
#[derive(Debug, Default)]
pub struct MidiParser {
    status: u8,
    needed: u8,
    count: u8,
    data: [u8; 2],
    sysex: bool,
}
impl MidiParser {
    pub fn feed(&mut self, byte: u8) -> Option<MidiMessage> {
        if byte >= 0xf8 {
            return Some(MidiMessage {
                bytes: [byte, 0, 0],
                len: 1,
            });
        }
        if byte & 0x80 != 0 {
            self.count = 0;
            self.sysex = byte == 0xf0;
            self.status = byte;
            self.needed = match byte {
                0x80..=0xbf | 0xe0..=0xef | 0xf2 => 2,
                0xc0..=0xdf | 0xf1 | 0xf3 => 1,
                _ => 0,
            };
            if byte == 0xf6 {
                return Some(MidiMessage {
                    bytes: [byte, 0, 0],
                    len: 1,
                });
            }
            return None;
        }
        if self.sysex || self.needed == 0 {
            return None;
        }
        self.data[usize::from(self.count)] = byte;
        self.count += 1;
        if self.count != self.needed {
            return None;
        }
        let message = MidiMessage {
            bytes: [
                self.status,
                self.data[0],
                if self.needed == 2 { self.data[1] } else { 0 },
            ],
            len: self.needed + 1,
        };
        self.count = 0;
        if self.status >= 0xf0 {
            self.status = 0;
            self.needed = 0;
        }
        Some(message)
    }
}
