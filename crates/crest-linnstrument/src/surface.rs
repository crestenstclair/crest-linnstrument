use crate::MidiMessage;
use crest_performance_core::{Error, Result, SlotState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Geometry {
    Pads200,
    Pads128,
}
impl Geometry {
    pub fn columns(self) -> u8 {
        match self {
            Self::Pads200 => 26,
            Self::Pads128 => 17,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadEvent {
    pub column: u8,
    pub row: u8,
    pub pressed: bool,
}
pub fn user_pad(message: MidiMessage, geometry: Geometry) -> Result<Option<PadEvent>> {
    let [status, column, velocity] = message.bytes();
    if !matches!(status & 0xf0, 0x80 | 0x90) {
        return Ok(None);
    }
    let row = status & 15;
    if row >= 8 || column >= geometry.columns() {
        return Err(Error::Invalid("user-mode coordinate"));
    }
    Ok(Some(PadEvent {
        column,
        row,
        pressed: status & 0xf0 == 0x90 && velocity > 0,
    }))
}
pub fn user_mode(enabled: bool) -> [MidiMessage; 4] {
    [
        MidiMessage::channel(0xb0, 0, 99, 1),
        MidiMessage::channel(0xb0, 0, 98, 117),
        MidiMessage::channel(0xb0, 0, 6, 0),
        MidiMessage::channel(0xb0, 0, 38, u8::from(enabled)),
    ]
}
/// LED intent only. A simulator must send this through firmware MIDI input and
/// display decoded hardware output. This buffer is never a displayed RGB frame.
pub struct LedRenderer {
    geometry: Geometry,
    previous: [[u8; 8]; 26],
}
impl LedRenderer {
    pub fn new(geometry: Geometry) -> Self {
        Self {
            geometry,
            previous: [[255; 8]; 26],
        }
    }
    pub fn invalidate(&mut self) {
        self.previous.fill([255; 8]);
    }
    /// Bounded by caller budget; each complete cell update is three ordered CCs.
    pub fn diff(&mut self, frame: &[[u8; 8]; 26], cell_budget: usize) -> Result<Vec<MidiMessage>> {
        if frame.iter().flatten().any(|color| *color > 11) {
            return Err(Error::Invalid("LED palette"));
        }
        let mut messages = Vec::with_capacity(cell_budget.min(208) * 3);
        for column in 0..self.geometry.columns() {
            self.diff_column(frame, column, cell_budget, &mut messages);
        }
        Ok(messages)
    }
    fn diff_column(
        &mut self,
        frame: &[[u8; 8]; 26],
        column: u8,
        budget: usize,
        messages: &mut Vec<MidiMessage>,
    ) {
        for (row, &color) in frame[usize::from(column)].iter().enumerate() {
            if messages.len() / 3 == budget {
                return;
            }
            if self.previous[usize::from(column)][row] == color {
                continue;
            }
            messages.extend([
                MidiMessage::channel(0xb0, 0, 20, column),
                MidiMessage::channel(0xb0, 0, 21, row as u8),
                MidiMessage::channel(0xb0, 0, 22, color),
            ]);
            self.previous[usize::from(column)][row] = color;
        }
    }
}
/// Stock User Firmware Mode palette: cyan distinguishes a muted running clip.
pub const MUTED_SLOT_COLOR: u8 = 4;

pub fn slot_color(state: SlotState) -> u8 {
    match state {
        SlotState::Empty => 0,
        SlotState::Recorded => 5,
        SlotState::Queued => 2,
        SlotState::Recording => 1,
        SlotState::Overdubbing => 6,
        SlotState::Playing => 3,
    }
}
