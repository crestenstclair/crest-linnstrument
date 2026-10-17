//! Shared MIDI and surface boundary for physical and firmware-backed devices.
mod midi;
mod mpe;
mod output;
pub use output::*;
mod note_input;
mod surface;
pub use midi::*;
pub use mpe::*;
pub use note_input::*;
pub use surface::*;
