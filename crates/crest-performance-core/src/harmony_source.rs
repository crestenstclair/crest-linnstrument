use crate::{BeatTime, Clip, Gesture, HarmonySettings};
use serde::{Deserialize, Serialize};

/// The harmonic intent used by following clips. Live chord input continues to
/// update selected harmony while a clip leader runs; stopping it restores that
/// selection. A leader without recorded chord gestures also uses the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum HarmonySource {
    Selected {},
    Clip { slot: usize },
}
impl Default for HarmonySource {
    fn default() -> Self {
        Self::Selected {}
    }
}

/// Control-thread index of recorded intent, independent of rendered note order.
/// Delayed strums/humanization and edits cannot move the original chord gesture.
#[derive(Default)]
pub(crate) struct HarmonyTimeline(Vec<(BeatTime, HarmonySettings)>);
impl HarmonyTimeline {
    pub(crate) fn from_clip(clip: Option<&Clip>) -> Self {
        let Some(clip) = clip else {
            return Self::default();
        };
        let mut changes: Vec<_> = clip
            .harmony_origin
            .iter()
            .filter(|origin| {
                matches!(origin.input.gesture, Gesture::On { .. }) && origin.input.at < clip.length
            })
            .map(|origin| (origin.input.at, origin.settings))
            .collect();
        // Stable sort preserves recorded order for simultaneous gestures. The
        // final gesture at a tick wins, independently of player launch order.
        changes.sort_by_key(|change| change.0);
        Self(changes)
    }
    pub(crate) fn at(&self, phase: BeatTime, repeated: bool) -> Option<HarmonySettings> {
        let index = self.0.partition_point(|change| change.0 <= phase);
        if index > 0 {
            Some(self.0[index - 1].1)
        } else if repeated {
            self.0.last().map(|change| change.1)
        } else {
            None
        }
    }
}
