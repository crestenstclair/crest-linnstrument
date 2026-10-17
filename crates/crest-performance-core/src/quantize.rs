//! Explicit, control-thread attack quantization. Whole lifetimes move together;
//! no curve resampling, duration changes, note merging or implicit recording grid.
use crate::*;
use std::collections::BTreeMap;

impl Clip {
    /// Move each attack to the nearest feasible multiple of `quantum`, measured
    /// from clip zero. Halfway ties move later. Durations and expression offsets
    /// stay exact, so the last usable grid point may precede the loop boundary.
    /// A zero grid or reused identity rejects rather than rewriting ownership.
    /// Historical harmony/pattern provenance and per-note rules are preserved.
    pub fn quantize_attacks(&self, quantum: BeatTime) -> Result<Self> {
        if quantum.0 == 0 {
            return Err(Error::Invalid("quantization grid must be nonzero"));
        }
        let track = self
            .events
            .first()
            .map_or(TrackId(0), |event| event.note.track);
        self.validate(track)?;
        let mut positions = BTreeMap::new();
        for note in self.notes()? {
            let start = quantized_start(&note, self.length, quantum);
            positions.insert(note.id, (note.start, start));
        }
        let mut clip = self.clone();
        if positions.values().all(|(before, after)| before == after) {
            return Ok(clip);
        }
        for event in &mut clip.events {
            let (before, after) = positions[&event.note];
            // Both bounds were validated for the complete lifetime, including
            // zero-duration notes and expression immediately before release.
            event.at = BeatTime(after.0 + (event.at.0 - before.0));
        }
        crate::edit::order_edited_events(&mut clip.events);
        clip.validate(track)?;
        Ok(clip)
    }
}

fn quantized_start(note: &Note, length: BeatTime, quantum: BeatTime) -> BeatTime {
    let last_expression = note
        .expression
        .points
        .last()
        .map_or(0, |point| point.offset.0);
    // Only releases can occur exactly at loop end. Preserve a closing expression
    // sample (and zero-duration attack) by choosing an earlier feasible grid.
    let latest = (length.0 - note.duration.0).min(length.0 - 1 - last_expression);
    let index = note.start.0 / quantum.0;
    let remainder = note.start.0 % quantum.0;
    // Comparing with the complementary distance avoids overflowing start+q/2.
    let rounded = index + u64::from(remainder >= quantum.0 - remainder);
    BeatTime(rounded.min(latest / quantum.0) * quantum.0)
}
