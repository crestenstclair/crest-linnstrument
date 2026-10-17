use crate::*;
use serde::{Deserialize, Serialize};

/// Lossless UI projection. Point offsets are relative to the note attack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpressionPoint {
    pub offset: BeatTime,
    pub value: Expression,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExpressionCurve {
    pub points: Vec<ExpressionPoint>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub id: NoteKey,
    pub key: u8,
    pub velocity: u8,
    pub release_velocity: u8,
    pub start: BeatTime,
    pub duration: BeatTime,
    pub expression: ExpressionCurve,
}
#[derive(Debug, Clone, Copy)]
pub struct NoteEdit {
    pub start: BeatTime,
    pub duration: BeatTime,
    pub key: u8,
}
impl Clip {
    /// Control-thread projection for a piano roll or tracker. Does not quantize.
    pub fn notes(&self) -> Result<Vec<Note>> {
        let mut notes: Vec<Note> = Vec::new();
        for event in &self.events {
            project_event(&mut notes, *event)?;
        }
        Ok(notes)
    }
    /// Explicit edit: moving retains curve offsets; shortening trims points
    /// at or beyond a shortened end. Moving or repitching preserves samples at
    /// the existing release. The engine retains the complete previous clip for undo.
    pub fn edit_note(&self, id: NoteKey, edit: NoteEdit) -> Result<Self> {
        if edit.key > 127
            || edit.duration.0 == 0
            || edit.start.checked_add(edit.duration)? > self.length
        {
            return Err(Error::Invalid("edited note bounds"));
        }
        self.validate(id.track)?;
        let original = self
            .notes()?
            .into_iter()
            .find(|note| note.id == id)
            .ok_or(Error::UnknownNote)?;
        let mut clip = self.clone();
        clip.events.retain_mut(|event| {
            if event.note != id {
                return true;
            }
            let offset = event.at.0 - original.start.0;
            match &mut event.gesture {
                Gesture::On { key, .. } => {
                    *key = edit.key;
                    event.at = edit.start;
                }
                Gesture::Expression { .. } => {
                    if edit.duration < original.duration && offset >= edit.duration.0 {
                        return false;
                    }
                    // The edited end was checked above. Retained offsets cannot
                    // exceed it, including samples exactly at a retained release.
                    event.at = BeatTime(edit.start.0 + offset);
                }
                Gesture::Off { .. } => event.at = BeatTime(edit.start.0 + edit.duration.0),
            }
            true
        });
        if edit.start != original.start || edit.duration != original.duration {
            order_edited_events(&mut clip.events);
        }
        clip.validate(id.track)?;
        Ok(clip)
    }
    pub(crate) fn from_notes(length: BeatTime, notes: &[Note]) -> Result<Self> {
        let mut events = Vec::new();
        for note in notes {
            append_note(&mut events, note)?;
        }
        events.sort_by_key(|e| {
            (
                e.at,
                if matches!(e.gesture, Gesture::Off { .. }) {
                    0
                } else {
                    1
                },
            )
        });
        let clip = Self {
            length,
            events,
            harmony_origin: Vec::new(),
            pattern_origin: Vec::new(),
            follow: None,
            conditions: Vec::new(),
            parameter_locks: Vec::new(),
        };
        if let Some(note) = notes.first() {
            clip.validate(note.id.track)?;
        }
        Ok(clip)
    }
    pub(crate) fn overdub(&self, take: &Clip) -> Result<Self> {
        // The existing raw stream is authoritative, including equal-time order,
        // zero-duration notes and expression immediately before a release.
        let mut clip = self.clone();
        append_overdub_take(&mut clip, take)?;
        clip.events.sort_by_key(|event| event.at);
        clip.conditions.sort_by_key(|entry| entry.note);
        clip.parameter_locks
            .sort_by_key(|entry| (entry.note, entry.parameter));
        clip.pattern_origin.extend_from_slice(&take.pattern_origin);
        clip.harmony_origin
            .extend(take.harmony_origin.iter().copied().map(|mut g| {
                g.input.at.0 %= self.length.0;
                g
            }));
        if let Some(event) = clip.events.first() {
            clip.validate(event.note.track)?;
        }
        Ok(clip)
    }
}
/// Fold only the new lifetimes. Original events precede new events at equal
/// timestamps, and stable sorting retains each stream's caller order. A new
/// identity is allocated for every attack, even when an input reuses its ID.
fn append_overdub_take(clip: &mut Clip, take: &Clip) -> Result<()> {
    let mut next = clip.events.iter().map(|e| e.note.note.0).max().unwrap_or(0);
    let mut held = std::collections::BTreeMap::new();
    for event in &take.events {
        let key = (event.note.source.0, event.note.track.0, event.note.note.0);
        if matches!(event.gesture, Gesture::On { .. }) {
            next = next
                .checked_add(1)
                .ok_or(Error::Capacity("clip note identities"))?;
            let mut note = event.note;
            note.note = NoteId(next);
            held.insert(key, (note, event.at.0, event.at.0 % clip.length.0));
            let condition = take.note_condition(event.note);
            if condition != (NoteCondition::Always {}) {
                clip.conditions.push(ConditionalNote { note, condition });
            }
            clip.parameter_locks.extend(
                take.note_parameters(event.note)
                    .iter()
                    .map(|entry| NoteParameterLock { note, ..*entry }),
            );
        }
        let &(note, original_start, loop_start) = held.get(&key).ok_or(Error::UnknownNote)?;
        let offset = event
            .at
            .0
            .checked_sub(original_start)
            .ok_or(Error::TimeOrder)?;
        let remaining = clip.length.0 - loop_start;
        // Only releases may reach the loop boundary. Other closing expression
        // remains untouched, including samples on a zero-duration note.
        if matches!(event.gesture, Gesture::Expression { .. }) && offset >= remaining {
            continue;
        }
        clip.events.push(InputEvent {
            at: BeatTime(loop_start + offset.min(remaining)),
            note,
            gesture: event.gesture,
        });
        if matches!(event.gesture, Gesture::Off { .. }) {
            held.remove(&key);
        }
    }
    Ok(())
}

/// Closing expression precedes its release; held-note releases precede new
/// attacks. Zero-duration notes keep their original On/Off ordering. A pitch-only
/// edit bypasses this pass and preserves the entire original event sequence.
pub(crate) fn order_edited_events(events: &mut [InputEvent]) {
    let key = |note: NoteKey| (note.source.0, note.track.0, note.note.0);
    let mut lifetimes = std::collections::BTreeMap::new();
    for event in events.iter() {
        let lifetime = lifetimes
            .entry(key(event.note))
            .or_insert((event.at, event.at));
        match event.gesture {
            Gesture::On { .. } => lifetime.0 = event.at,
            Gesture::Off { .. } => lifetime.1 = event.at,
            _ => {}
        }
    }
    events.sort_by_key(|event| {
        let (start, end) = lifetimes[&key(event.note)];
        let phase = if start == end {
            // A newly arriving zero-duration lifetime must not delay releases
            // of voices already held at this timestamp. Keep its own order.
            1
        } else if event.at == end {
            0
        } else {
            match event.gesture {
                Gesture::On { .. } => 3,
                Gesture::Expression { .. } if event.at == start => 4,
                _ => 2,
            }
        };
        (event.at, phase)
    });
}

fn project_event(notes: &mut Vec<Note>, event: InputEvent) -> Result<()> {
    if let Gesture::On {
        key,
        velocity,
        expression,
    } = event.gesture
    {
        if notes.iter().any(|n| n.id == event.note) {
            return Err(Error::Invalid("reused note identity inside clip"));
        }
        notes.push(Note {
            id: event.note,
            key,
            velocity,
            release_velocity: 0,
            start: event.at,
            duration: BeatTime(0),
            expression: ExpressionCurve {
                points: vec![ExpressionPoint {
                    offset: BeatTime(0),
                    value: expression,
                }],
            },
        });
        return Ok(());
    }
    let note = notes
        .iter_mut()
        .find(|n| n.id == event.note)
        .ok_or(Error::UnknownNote)?;
    let offset = BeatTime(
        event
            .at
            .0
            .checked_sub(note.start.0)
            .ok_or(Error::TimeOrder)?,
    );
    match event.gesture {
        Gesture::Expression { expression } => note.expression.points.push(ExpressionPoint {
            offset,
            value: expression,
        }),
        Gesture::Off { velocity } => {
            note.duration = offset;
            note.release_velocity = velocity;
        }
        _ => {}
    }
    Ok(())
}
fn append_note(events: &mut Vec<InputEvent>, note: &Note) -> Result<()> {
    let initial = note
        .expression
        .points
        .first()
        .ok_or(Error::Invalid("note lacks initial expression"))?;
    if initial.offset.0 != 0 || note.duration.0 == 0 {
        return Err(Error::Invalid("note duration or curve origin"));
    }
    events.push(InputEvent {
        at: note.start,
        note: note.id,
        gesture: Gesture::On {
            key: note.key,
            velocity: note.velocity,
            expression: initial.value,
        },
    });
    for point in note.expression.points.iter().skip(1) {
        if point.offset >= note.duration {
            return Err(Error::Invalid("expression after note end"));
        }
        events.push(InputEvent {
            at: note.start.checked_add(point.offset)?,
            note: note.id,
            gesture: Gesture::Expression {
                expression: point.value,
            },
        });
    }
    events.push(InputEvent {
        at: note.start.checked_add(note.duration)?,
        note: note.id,
        gesture: Gesture::Off {
            velocity: note.release_velocity,
        },
    });
    Ok(())
}

pub(crate) fn retrospective_clip(
    events: impl Iterator<Item = InputEvent>,
    track: TrackId,
    start: BeatTime,
    end: BeatTime,
) -> Result<Clip> {
    let mut held: Vec<InputEvent> = Vec::new();
    let mut selected = Vec::new();
    let mut began = false;
    for event in events.filter(|e| e.note.track == track) {
        if event.at < start {
            remember_held(&mut held, event);
            continue;
        }
        if !began {
            selected.extend(held.iter().map(|e| InputEvent {
                at: BeatTime(0),
                ..*e
            }));
            began = true;
        }
        selected.push(InputEvent {
            at: BeatTime(event.at.0 - start.0),
            ..event
        });
        remember_held(&mut held, event);
    }
    if !began {
        selected.extend(held.iter().map(|e| InputEvent {
            at: BeatTime(0),
            ..*e
        }));
    }
    let length = BeatTime(end.0 - start.0);
    for event in held {
        selected.push(InputEvent {
            at: length,
            note: event.note,
            gesture: Gesture::Off { velocity: 0 },
        });
    }
    let clip = Clip {
        length,
        events: selected,
        harmony_origin: Vec::new(),
        pattern_origin: Vec::new(),
        follow: None,
        conditions: Vec::new(),
        parameter_locks: Vec::new(),
    };
    clip.validate(track)?;
    Ok(clip)
}
fn remember_held(held: &mut Vec<InputEvent>, event: InputEvent) {
    match event.gesture {
        Gesture::On { .. } => held.push(event),
        Gesture::Off { .. } => held.retain(|old| old.note != event.note),
        Gesture::Expression { expression } => {
            if let Some(previous) = held.iter_mut().find(|old| old.note == event.note)
                && let Gesture::On { key, velocity, .. } = previous.gesture
            {
                previous.gesture = Gesture::On {
                    key,
                    velocity,
                    expression,
                };
            }
        }
    }
}
