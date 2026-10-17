use super::*;

impl Engine {
    pub(super) fn slot_transpose(&self, slot: usize) -> i8 {
        self.players
            .iter()
            .find(|player| player.slot == slot)
            .map_or(0, |player| player.transpose)
    }

    /// Validate the complete currently resolved phrase before changing its
    /// future attacks. Restoring zero always works, including a late release
    /// after Stop. A later harmony change is checked again at each attack.
    pub(super) fn set_clip_transpose(&mut self, slot: usize, semitones: i8) -> Result<()> {
        let clip_slot = self.session.slots.get(slot).ok_or(Error::NotFound)?;
        let Some(index) = self.players.iter().position(|player| player.slot == slot) else {
            return if semitones == 0 {
                Ok(())
            } else {
                Err(Error::State("launch clip before transposing"))
            };
        };
        if semitones != 0 {
            let clip = clip_slot.clip.as_ref().ok_or(Error::NotFound)?;
            let harmony = self.effective_harmony_at(self.at);
            for event in &clip.events {
                if let Gesture::On { key, .. } = event.gesture {
                    playback_pitch(key, clip.follow, harmony, semitones)?;
                }
            }
        }
        self.players[index].transpose = semitones;
        Ok(())
    }
}

pub(super) fn playback_pitch(
    key: u8,
    follow: Option<HarmonyFollow>,
    harmony: HarmonySettings,
    semitones: i8,
) -> Result<u8> {
    let key = match follow {
        Some(follow) => follow.pitch(key, harmony)?,
        None => key,
    };
    let shifted = i16::from(key) + i16::from(semitones);
    if !(0..=127).contains(&shifted) {
        return Err(Error::Invalid("transposed clip pitch outside MIDI range"));
    }
    Ok(shifted as u8)
}
