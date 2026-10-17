//! A/B parameter interpolation, independent of notes, clip scenes and sound DSP.
//! Hosts advertise the addresses they can accept; values use a normalized u16.
use crate::{BeatTime, DestinationId, Error, Result};
use serde::{Deserialize, Serialize};

pub const MAX_MORPH_PARAMETERS: usize = 16;

/// An instrument-defined stable parameter reference, not a MIDI controller or
/// an index into a GUI. The host resolves it at the destination boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterAddress {
    pub destination: DestinationId,
    pub parameter: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MorphParameter {
    pub address: ParameterAddress,
    pub a: u16,
    pub b: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "Vec<MorphParameter>", into = "Vec<MorphParameter>")]
pub struct SceneMorph(Vec<MorphParameter>);
impl TryFrom<Vec<MorphParameter>> for SceneMorph {
    type Error = Error;
    fn try_from(parameters: Vec<MorphParameter>) -> Result<Self> {
        if parameters.is_empty() || parameters.len() > MAX_MORPH_PARAMETERS {
            return Err(Error::Invalid("scene morph needs 1–16 parameters"));
        }
        let mut previous = None;
        for parameter in &parameters {
            if previous.is_some_and(|address| address >= parameter.address) {
                return Err(Error::Invalid(
                    "scene parameters must have unique sorted addresses",
                ));
            }
            previous = Some(parameter.address);
        }
        Ok(Self(parameters))
    }
}
impl From<SceneMorph> for Vec<MorphParameter> {
    fn from(scene: SceneMorph) -> Self {
        scene.0
    }
}
impl SceneMorph {
    pub fn parameters(&self) -> &[MorphParameter] {
        &self.0
    }
    /// Linear normalized interpolation, rounded to nearest u16. Both endpoints
    /// are exact, descending values work, and each address appears exactly once.
    /// This iterator allocates nothing. Hosts choose DSP taper/units separately.
    pub fn render(&self, at: BeatTime, position: u16) -> impl Iterator<Item = ParameterEvent> + '_ {
        self.0.iter().map(move |parameter| {
            let weighted = u64::from(parameter.a) * u64::from(u16::MAX - position)
                + u64::from(parameter.b) * u64::from(position);
            ParameterEvent {
                at,
                voice: None,
                address: parameter.address,
                value: ((weighted + 32_767) / 65_535) as u16,
            }
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParameterEvent {
    pub at: BeatTime,
    /// None updates destination-wide state. Some binds this attack setting to
    /// exactly the matching OutputEvent voice, before its Note On. Never apply
    /// it globally; discard it when that voice ends. No previous voice value
    /// may carry into a fresh voice without a lock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<crate::VoiceId>,
    pub address: ParameterAddress,
    pub value: u16,
}
