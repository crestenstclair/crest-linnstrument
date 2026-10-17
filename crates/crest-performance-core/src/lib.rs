//! Deterministic musical state. No device, filesystem, GUI, or sound engine dependency.
mod condition;
mod edit;
pub use condition::*;
mod engine;
mod harmony;
mod learned_chord;
pub use learned_chord::*;
mod pattern;
pub use edit::*;
pub use harmony::*;
pub use pattern::*;
mod model;
mod time;
pub use engine::*;
pub use model::*;
pub use time::*;

pub const API_VERSION: u32 = 6;
pub const SESSION_VERSION: u32 = 6;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("unsupported version {0}")]
    Version(u32),
    #[error("invalid value: {0}")]
    Invalid(&'static str),
    #[error("time must not move backwards")]
    TimeOrder,
    #[error("resource capacity reached: {0}")]
    Capacity(&'static str),
    #[error("note identity already active")]
    DuplicateNote,
    #[error("note identity is not active")]
    UnknownNote,
    #[error("operation unavailable in current state: {0}")]
    State(&'static str),
    #[error("unknown track or clip slot")]
    NotFound,
}
pub type Result<T> = std::result::Result<T, Error>;
