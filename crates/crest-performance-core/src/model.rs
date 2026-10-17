use crate::{
    BeatTime, Error, HarmonyFollow, HarmonyGesture, HarmonySettings, PatternOrigin, Result,
    SESSION_VERSION,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

macro_rules! identity {
    ($name:ident, $kind:ty) => {
        #[derive(
            Debug,
            Default,
            Clone,
            Copy,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
        )]
        pub struct $name(pub $kind);
    };
}
identity!(SourceId, u32);
identity!(TrackId, u32);
identity!(DestinationId, u32);
identity!(NoteId, u64);
identity!(VoiceId, u64);

/// Input ownership is independent of MIDI channels and pitch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoteKey {
    pub source: SourceId,
    pub track: TrackId,
    pub note: NoteId,
}

/// Pitch is a signed offset in millionths of a semitone; pressure/timbre use
/// the full u16 range. Absolute pitch is the NoteOn key plus this offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Expression {
    pub pitch: i32,
    pub pressure: u16,
    pub timbre: u16,
}
impl Default for Expression {
    fn default() -> Self {
        Self {
            pitch: 0,
            pressure: 0,
            timbre: 32_768,
        }
    }
}
impl Expression {
    pub fn validate(self) -> Result<()> {
        if !(-96_000_000..=96_000_000).contains(&self.pitch) {
            return Err(Error::Invalid("pitch expression"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Gesture {
    On {
        key: u8,
        velocity: u8,
        expression: Expression,
    },
    Expression {
        expression: Expression,
    },
    Off {
        velocity: u8,
    },
}
impl Gesture {
    pub fn validate(self) -> Result<()> {
        match self {
            Self::On {
                key,
                velocity,
                expression,
            } => {
                if key > 127 || !(1..=127).contains(&velocity) {
                    return Err(Error::Invalid("note on"));
                }
                expression.validate()
            }
            Self::Expression { expression } => expression.validate(),
            Self::Off { velocity } if velocity > 127 => Err(Error::Invalid("release velocity")),
            _ => Ok(()),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputEvent {
    pub at: BeatTime,
    pub note: NoteKey,
    pub gesture: Gesture,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputEvent {
    pub at: BeatTime,
    pub destination: DestinationId,
    pub track: TrackId,
    pub voice: VoiceId,
    pub gesture: Gesture,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstrumentTarget {
    pub destination: DestinationId,
    pub reference: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Track {
    pub id: TrackId,
    pub name: String,
    pub instrument: InstrumentTarget,
}

/// Authoritative lossless gesture stream. Offsets and original note identities
/// survive serialization; playback allocates fresh VoiceIds at each iteration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Clip {
    pub length: BeatTime,
    pub events: Vec<InputEvent>,
    /// Historical generating gestures; rendered events remain authoritative after edits.
    #[serde(default)]
    pub harmony_origin: Vec<HarmonyGesture>,
    #[serde(default)]
    pub pattern_origin: Vec<PatternOrigin>,
    #[serde(default)]
    pub follow: Option<HarmonyFollow>,
}
impl Clip {
    pub fn validate(&self, track: TrackId) -> Result<()> {
        if self.length.0 == 0 {
            return Err(Error::Invalid("empty loop length"));
        }
        for origin in &self.pattern_origin {
            origin.validate()?;
        }
        if let Some(follow) = self.follow {
            follow.reference.validate()?;
        }
        for origin in &self.harmony_origin {
            origin.validate()?;
            if origin.input.at > self.length {
                return Err(Error::Invalid("harmony origin beyond loop"));
            }
        }
        let mut held = Vec::new();
        let mut last = BeatTime(0);
        for event in &self.events {
            event.gesture.validate()?;
            if event.at < last || event.at > self.length || event.note.track != track {
                return Err(Error::Invalid("clip event ordering or track"));
            }
            if event.at == self.length && !matches!(event.gesture, Gesture::Off { .. }) {
                return Err(Error::Invalid("only releases allowed at loop end"));
            }
            validate_lifecycle(&mut held, event.note, event.gesture)?;
            last = event.at;
        }
        if !held.is_empty() {
            return Err(Error::Invalid("clip has unreleased notes"));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClipSlot {
    pub track: TrackId,
    pub clip: Option<Clip>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Session {
    pub version: u32,
    pub tracks: Vec<Track>,
    pub slots: Vec<ClipSlot>,
    #[serde(default)]
    pub harmony: HarmonySettings,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            version: SESSION_VERSION,
            harmony: HarmonySettings::default(),
            tracks: vec![Track {
                id: TrackId(1),
                name: "LinnStrument".into(),
                instrument: InstrumentTarget {
                    destination: DestinationId(1),
                    reference: "capture://main".into(),
                },
            }],
            slots: vec![
                ClipSlot {
                    track: TrackId(1),
                    clip: None
                };
                8
            ],
        }
    }
}
impl Session {
    /// Lossless upgrade of the pre-harmony schema. Unknown versions fail closed.
    pub fn migrate(mut self) -> Result<Self> {
        if self.version == 1 {
            if self.harmony != HarmonySettings::default()
                || self.slots.iter().any(|s| {
                    s.clip.as_ref().is_some_and(|c| {
                        !c.harmony_origin.is_empty()
                            || !c.pattern_origin.is_empty()
                            || c.follow.is_some()
                    })
                })
            {
                return Err(Error::Invalid("v1 session contains v2 harmony fields"));
            }
            self.version = SESSION_VERSION;
        }
        self.validate()?;
        Ok(self)
    }
    pub fn validate(&self) -> Result<()> {
        if self.version != SESSION_VERSION {
            return Err(Error::Version(self.version));
        }
        self.harmony.validate()?;
        if self
            .harmony
            .bass_track
            .is_some_and(|id| !self.tracks.iter().any(|t| t.id == id))
        {
            return Err(Error::NotFound);
        }
        let mut ids = BTreeMap::new();
        for track in &self.tracks {
            if ids.insert(track.id, ()).is_some() {
                return Err(Error::Invalid("duplicate track"));
            }
        }
        for slot in &self.slots {
            self.validate_slot(slot)?;
        }
        Ok(())
    }
    fn validate_slot(&self, slot: &ClipSlot) -> Result<()> {
        if !self.tracks.iter().any(|t| t.id == slot.track) {
            return Err(Error::NotFound);
        }
        if let Some(clip) = &slot.clip {
            clip.validate(slot.track)?;
        }
        Ok(())
    }
}

pub(crate) fn validate_lifecycle(
    held: &mut Vec<NoteKey>,
    key: NoteKey,
    gesture: Gesture,
) -> Result<()> {
    let position = held.iter().position(|item| *item == key);
    match (gesture, position) {
        (Gesture::On { .. }, Some(_)) => Err(Error::DuplicateNote),
        (Gesture::On { .. }, None) => {
            held.push(key);
            Ok(())
        }
        (Gesture::Off { .. }, Some(index)) => {
            held.remove(index);
            Ok(())
        }
        (Gesture::Expression { .. }, Some(_)) => Ok(()),
        (_, None) => Err(Error::UnknownNote),
    }
}
