//! Shared MIDI and surface boundary for physical and firmware-backed devices.
mod midi;
mod mpe;
mod output;
pub use output::*;
mod note_input;
mod surface;
mod user_row;
pub use midi::*;
pub use mpe::*;
pub use note_input::*;
pub use surface::*;
pub use user_row::UserRowX;

mod user_notes;
pub use user_notes::UserNoteInput;
