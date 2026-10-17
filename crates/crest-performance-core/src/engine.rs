mod articulation;
mod harmony;
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub voices: usize,
    pub output_events: usize,
    pub recorded_events: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            voices: 256,
            output_events: 4096,
            recorded_events: 65_536,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SurfaceMode {
    Play,
    Chords,
    Clips,
}
#[derive(Debug, Clone, Copy)]
pub enum Command {
    BeginRecord {
        slot: usize,
    },
    BeginOverdub {
        slot: usize,
    },
    Duplicate {
        from: usize,
        to: usize,
    },
    EditNote {
        slot: usize,
        note: NoteKey,
        edit: NoteEdit,
    },
    SetNoteCondition {
        slot: usize,
        note: NoteKey,
        condition: NoteCondition,
    },
    /// Momentary host control. Affects future attacks only, at engine time.
    SetFill(bool),
    CaptureRecent {
        slot: usize,
        length: BeatTime,
    },
    CaptureRecentAt {
        slot: usize,
        length: BeatTime,
        cutoff: BeatTime,
    },
    FinishRecord,
    CancelRecord,
    Launch {
        slot: usize,
        quantum: BeatTime,
    },
    StopSlot {
        slot: usize,
    },
    Stop,
    SetMode(SurfaceMode),
    SetHarmony(HarmonySettings),
    LearnChord {
        source: SourceId,
        track: TrackId,
        degree: u8,
        root: Option<u8>,
    },
    ClearLearnedChord {
        degree: u8,
    },
    Generate {
        slot: usize,
        settings: PatternSettings,
    },
    FollowHarmony {
        slot: usize,
        enabled: bool,
    },
    FreezeHarmony {
        slot: usize,
    },
    Undo,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SlotState {
    Empty,
    Recorded,
    Queued,
    Recording,
    Overdubbing,
    Playing,
}
#[derive(Debug, Clone, Serialize)]
pub struct PerformanceSnapshot {
    pub api_version: u32,
    pub at: BeatTime,
    pub mode: SurfaceMode,
    pub fill: bool,
    pub active_voices: usize,
    pub voice_capacity: usize,
    pub slots: Vec<SlotState>,
    pub harmony: HarmonySettings,
    pub last_voicing: Option<Voicing>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Owner {
    Live(NoteKey),
    Harmony(NoteKey),
    Clip(usize, NoteKey),
}
#[derive(Debug, Clone, Copy)]
struct ActiveVoice {
    owner: Owner,
    id: VoiceId,
    track: TrackId,
    key: u8,
    destination: DestinationId,
    rendered_note: NoteKey,
    expressive: bool,
    pending_attack: Option<InputEvent>,
    arpeggio: Option<articulation::RepeatingTone>,
}
#[derive(Debug)]
struct Player {
    slot: usize,
    loop_start: BeatTime,
    scheduled_start: BeatTime,
    cursor: usize,
}
#[derive(Debug)]
struct Recording {
    previous: Option<Clip>,
    slot: usize,
    start: BeatTime,
    events: Vec<InputEvent>,
    held: Vec<NoteKey>,
    harmony_origin: Vec<HarmonyGesture>,
}

enum UndoState {
    Clip {
        slot: usize,
        clip: Option<Box<Clip>>,
    },
    LearnedChord {
        degree: u8,
        chord: Option<LearnedChord>,
    },
}

impl Recording {
    fn clip_at(&self, cutoff: BeatTime, track: TrackId) -> Result<Clip> {
        let length = BeatTime(cutoff.0 - self.start.0);
        if length.0 == 0
            || self
                .events
                .last()
                .is_some_and(|e| e.at == length && !matches!(e.gesture, Gesture::Off { .. }))
        {
            return Err(Error::State("advance past final gesture before finishing"));
        }
        let mut events = self.events.clone();
        for note in &self.held {
            events.push(InputEvent {
                at: length,
                note: *note,
                gesture: Gesture::Off { velocity: 0 },
            });
        }
        if events.is_empty() {
            return Err(Error::State("take has no notes; cancel recording"));
        }
        let mut clip = Clip {
            length,
            events,
            harmony_origin: self.harmony_origin.clone(),
            pattern_origin: Vec::new(),
            follow: None,
            conditions: Vec::new(),
        };
        clip.validate(track)?;
        if let Some(previous) = &self.previous {
            clip = previous.overdub(&clip)?;
        }
        Ok(clip)
    }
}

/// Single-owner engine, driven exclusively by host time. Musical methods use
/// bounded preallocated queues. Session edits/record setup/snapshots are control
/// work and must run outside an audio callback. No locks or I/O are performed.
pub struct Engine {
    session: Session,
    limits: Limits,
    at: BeatTime,
    next_voice: u64,
    mode: SurfaceMode,
    fill: bool,
    active: Vec<ActiveVoice>,
    players: Vec<Player>,
    output: VecDeque<OutputEvent>,
    recording: Option<Recording>,
    undo: Option<UndoState>,
    recent: VecDeque<InputEvent>,
    history_floor: BeatTime,
    recent_harmony: VecDeque<HarmonyGesture>,
    last_voicing: Option<Voicing>,
}
impl Engine {
    pub fn new(api_version: u32, session: Session, limits: Limits) -> Result<Self> {
        if api_version != API_VERSION {
            return Err(Error::Version(api_version));
        }
        let session = session.migrate()?;
        if limits.voices == 0
            || limits.output_events < limits.voices
            || limits.recorded_events < limits.voices
        {
            return Err(Error::Invalid("limits must reserve room for all releases"));
        }
        Ok(Self {
            players: Vec::with_capacity(session.slots.len()),
            session,
            limits,
            at: BeatTime(0),
            next_voice: 1,
            mode: SurfaceMode::Play,
            fill: false,
            active: Vec::with_capacity(limits.voices),
            output: VecDeque::with_capacity(limits.output_events),
            recording: None,
            undo: None,
            recent: VecDeque::with_capacity(limits.recorded_events),
            history_floor: BeatTime(0),
            recent_harmony: VecDeque::with_capacity(limits.recorded_events),
            last_voicing: None,
        })
    }
    pub fn position(&self) -> BeatTime {
        self.at
    }
    /// Initial boundary chosen by the latest accepted launch of this slot.
    /// Remains unchanged across loop iterations; stopped/unlaunched slots return
    /// None. Hosts may map it to audio time for queued/playing feedback even
    /// when their scheduling engine has advanced ahead of the audible timeline.
    pub fn slot_launch_time(&self, slot: usize) -> Result<Option<BeatTime>> {
        if slot >= self.session.slots.len() {
            return Err(Error::NotFound);
        }
        Ok(self
            .players
            .iter()
            .find(|player| player.slot == slot)
            .map(|player| player.scheduled_start))
    }
    pub fn session(&self) -> &Session {
        &self.session
    }
    /// Returns committed state only. Finish or cancel the active take first.
    pub fn save_session(&self) -> Result<Session> {
        if self.recording.is_some() {
            return Err(Error::State("finish recording before saving"));
        }
        Ok(self.session.clone())
    }
    /// Control-thread recovery snapshot. Closes held recorded notes one tick
    /// after the current engine position in the returned session only. Playback,
    /// MIDI output, live ownership and the active take are unchanged. Reopening
    /// this session restores the captured prefix as a stopped, committed clip.
    /// An armed/empty take leaves the previously committed slot intact.
    pub fn checkpoint_session(&self) -> Result<Session> {
        let cutoff = if self
            .recording
            .as_ref()
            .is_some_and(|take| !take.events.is_empty())
        {
            self.at.checked_add(BeatTime(1))?
        } else {
            self.at
        };
        self.checkpoint_session_at(cutoff)
    }
    /// Checkpoint at an explicit host-clock cutoff without advancing input time.
    /// A nonempty take requires a cutoff after the current engine position, so
    /// every accepted attack and expression sample precedes the copied release.
    pub fn checkpoint_session_at(&self, cutoff: BeatTime) -> Result<Session> {
        let mut session = self.session.clone();
        if let Some(recording) = self
            .recording
            .as_ref()
            .filter(|take| !take.events.is_empty())
        {
            if cutoff <= self.at {
                return Err(Error::TimeOrder);
            }
            let clip = recording.clip_at(cutoff, session.slots[recording.slot].track)?;
            session.slots[recording.slot].clip = Some(clip);
        }
        session.validate()?;
        Ok(session)
    }
    pub fn drain_output(&mut self) -> impl Iterator<Item = OutputEvent> + '_ {
        self.output.drain(..)
    }
    pub fn snapshot(&self) -> PerformanceSnapshot {
        PerformanceSnapshot {
            api_version: API_VERSION,
            at: self.at,
            mode: self.mode,
            fill: self.fill,
            active_voices: self.active.len(),
            voice_capacity: self.limits.voices,
            harmony: self.session.harmony,
            last_voicing: self.last_voicing,
            slots: (0..self.session.slots.len())
                .map(|slot| self.slot_state(slot))
                .collect(),
        }
    }
    fn slot_state(&self, slot: usize) -> SlotState {
        if self.recording.as_ref().is_some_and(|r| r.slot == slot) {
            return if self
                .recording
                .as_ref()
                .is_some_and(|r| r.previous.is_some())
            {
                SlotState::Overdubbing
            } else {
                SlotState::Recording
            };
        }
        if let Some(player) = self.players.iter().find(|p| p.slot == slot) {
            return if player.scheduled_start > self.at {
                SlotState::Queued
            } else {
                SlotState::Playing
            };
        }
        if self.session.slots[slot].clip.is_some() {
            SlotState::Recorded
        } else {
            SlotState::Empty
        }
    }
    /// Processes events through `target`, inclusively. On output/voice pressure,
    /// earlier events remain committed; time stops at the last completed event.
    /// Drain output and retry the same target. No event or release is discarded.
    pub fn advance(&mut self, target: BeatTime) -> Result<()> {
        if target < self.at {
            return Err(Error::TimeOrder);
        }
        loop {
            let playback = self.next_playback_event(target)?;
            let pending = self.next_harmony_event(target);
            if let Some((index, event)) = pending
                && playback.is_none_or(|(_, played)| {
                    (event.at, release_order(event.gesture))
                        < (played.at, release_order(played.gesture))
                })
            {
                self.perform_harmony_event(index, event)?;
                self.at = event.at;
                continue;
            }
            let Some((player_index, event)) = playback else {
                break;
            };
            self.play_clip_event(player_index, event)?;
            self.at = event.at;
            self.players[player_index].cursor += 1;
            self.wrap_player(player_index)?;
        }
        self.at = target;
        Ok(())
    }
    fn play_clip_event(&mut self, player_index: usize, mut event: InputEvent) -> Result<()> {
        let slot = self.players[player_index].slot;
        let clip = self.session.slots[slot]
            .clip
            .as_ref()
            .ok_or(Error::NotFound)?;
        let owner = Owner::Clip(slot, event.note);
        let condition = clip.note_condition(event.note);
        let play = match event.gesture {
            Gesture::On { .. } => {
                let iteration = (self.players[player_index].loop_start.0
                    - self.players[player_index].scheduled_start.0)
                    / clip.length.0;
                condition.accepts_validated(iteration, self.fill)
            }
            // Fill changes cannot suppress a previously accepted release or
            // introduce expression for a skipped attack. Unconditional clips
            // retain strict lifecycle errors rather than masking corruption.
            _ => {
                condition == (NoteCondition::Always {})
                    || self.active.iter().any(|voice| voice.owner == owner)
            }
        };
        if play
            && let (
                Some(follow),
                Gesture::On {
                    key,
                    velocity,
                    expression,
                },
            ) = (clip.follow, event.gesture)
        {
            event.gesture = Gesture::On {
                key: follow.pitch(key, self.session.harmony)?,
                velocity,
                expression,
            };
        }
        if play {
            self.emit(owner, event)?;
        }
        Ok(())
    }
    fn next_playback_event(&self, target: BeatTime) -> Result<Option<(usize, InputEvent)>> {
        let mut next: Option<(usize, InputEvent)> = None;
        for (index, player) in self.players.iter().enumerate() {
            let clip = self.session.slots[player.slot]
                .clip
                .as_ref()
                .ok_or(Error::NotFound)?;
            let mut event = clip.events[player.cursor];
            event.at = player.loop_start.checked_add(event.at)?;
            if event.at > target {
                continue;
            }
            if next.is_none_or(|(_, old)| {
                (event.at, release_order(event.gesture)) < (old.at, release_order(old.gesture))
            }) {
                next = Some((index, event));
            }
        }
        Ok(next)
    }
    fn wrap_player(&mut self, index: usize) -> Result<()> {
        let player = &mut self.players[index];
        let clip = self.session.slots[player.slot]
            .clip
            .as_ref()
            .ok_or(Error::NotFound)?;
        if player.cursor == clip.events.len() {
            if let Ok(next) = player.loop_start.checked_add(clip.length) {
                player.loop_start = next;
                player.cursor = 0;
            } else {
                self.players.remove(index);
            }
        }
        Ok(())
    }
    /// Call advance(event.at) before accepting input. Equal-time live events
    /// preserve caller order, following already scheduled playback at that time.
    pub fn input(&mut self, event: InputEvent) -> Result<()> {
        if event.at != self.at {
            return Err(Error::TimeOrder);
        }
        event.gesture.validate()?;
        if event.note.source == GENERATED_SOURCE {
            return Err(Error::Invalid("reserved generated source"));
        }
        if self.mode == SurfaceMode::Chords {
            return self.harmony_input(event);
        }
        self.check_record_capacity(event)?;
        self.emit(Owner::Live(event.note), event)?;
        self.capture(event);
        self.remember(event);
        Ok(())
    }
    fn emit(&mut self, owner: Owner, event: InputEvent) -> Result<()> {
        if self.output.len() == self.limits.output_events {
            return Err(Error::Capacity("output; drain and retry"));
        }
        let position = self.active.iter().position(|voice| voice.owner == owner);
        let voice = match (event.gesture, position) {
            (Gesture::On { .. }, Some(_)) => return Err(Error::DuplicateNote),
            (Gesture::On { key, .. }, None) => self.allocate_voice(owner, event.note.track, key)?,
            (Gesture::Off { .. }, Some(index)) => self.active.remove(index),
            (Gesture::Expression { .. }, Some(index)) => self.active[index],
            (_, None) => return Err(Error::UnknownNote),
        };
        self.output.push_back(OutputEvent {
            at: event.at,
            destination: voice.destination,
            track: voice.track,
            voice: voice.id,
            gesture: event.gesture,
        });
        Ok(())
    }
    fn allocate_voice(&mut self, owner: Owner, track: TrackId, key: u8) -> Result<ActiveVoice> {
        if self.active.len() == self.limits.voices {
            return Err(Error::Capacity("voices; new notes rejected"));
        }
        let destination = self
            .session
            .tracks
            .iter()
            .find(|t| t.id == track)
            .ok_or(Error::NotFound)?
            .instrument
            .destination;
        let next = self
            .next_voice
            .checked_add(1)
            .ok_or(Error::Capacity("voice identities"))?;
        let voice = ActiveVoice {
            owner,
            id: VoiceId(self.next_voice),
            track,
            key,
            destination,
            rendered_note: match owner {
                Owner::Live(note) | Owner::Clip(_, note) | Owner::Harmony(note) => note,
            },
            expressive: true,
            pending_attack: None,
            arpeggio: None,
        };
        self.next_voice = next;
        self.active.push(voice);
        Ok(voice)
    }
    fn check_record_capacity(&self, event: InputEvent) -> Result<()> {
        let Some(recording) = &self.recording else {
            return Ok(());
        };
        if event.note.track != self.session.slots[recording.slot].track {
            return Ok(());
        }
        if !matches!(event.gesture, Gesture::On { .. }) && !recording.held.contains(&event.note) {
            return Ok(());
        }
        let held = recording.held.len();
        let reserved = match event.gesture {
            Gesture::On { .. } => held + 1,
            Gesture::Off { .. } if recording.held.contains(&event.note) => held - 1,
            _ => held,
        };
        if recording.events.len() + 1 + reserved + self.pending_record_cost(Some(event.note))
            > self.limits.recorded_events
        {
            return Err(Error::Capacity("recording; finish or cancel take"));
        }
        Ok(())
    }
    fn capture(&mut self, mut event: InputEvent) -> bool {
        let Some(recording) = &mut self.recording else {
            return false;
        };
        if event.note.track != self.session.slots[recording.slot].track {
            return false;
        }
        if matches!(event.gesture, Gesture::On { .. }) {
            recording.held.push(event.note);
        } else if !recording.held.contains(&event.note) {
            return false;
        }
        if matches!(event.gesture, Gesture::Off { .. }) {
            recording.held.retain(|key| *key != event.note);
        }
        event.at.0 -= recording.start.0;
        recording.events.push(event);
        true
    }
    pub fn command(&mut self, command: Command) -> Result<()> {
        match command {
            Command::BeginRecord { slot } => self.begin_record(slot),
            Command::BeginOverdub { slot } => self.begin_overdub(slot),
            Command::Duplicate { from, to } => self.duplicate(from, to),
            Command::EditNote { slot, note, edit } => self.edit_note(slot, note, edit),
            Command::SetNoteCondition {
                slot,
                note,
                condition,
            } => self.set_note_condition(slot, note, condition),
            Command::SetFill(fill) => {
                self.fill = fill;
                Ok(())
            }
            Command::CaptureRecent { slot, length } => self.capture_recent(slot, length, self.at),
            Command::CaptureRecentAt {
                slot,
                length,
                cutoff,
            } => self.capture_recent_at(slot, length, cutoff),
            Command::FinishRecord => self.finish_record(),
            Command::CancelRecord => {
                self.recording = None;
                Ok(())
            }
            Command::Launch { slot, quantum } => self.launch(slot, quantum),
            Command::StopSlot { slot } => self.stop_slot(slot),
            Command::Stop => self.stop(),
            Command::SetMode(mode) => {
                if self.recording.is_some() {
                    return Err(Error::State(
                        "finish or cancel recording before changing mode",
                    ));
                }
                self.release_matching(|owner| matches!(owner, Owner::Live(_) | Owner::Harmony(_)))?;
                self.mode = mode;
                self.fill = false;
                Ok(())
            }
            Command::SetHarmony(settings) => self.set_harmony(settings),
            Command::LearnChord {
                source,
                track,
                degree,
                root,
            } => self.learn_chord(source, track, degree, root),
            Command::ClearLearnedChord { degree } => self.replace_learned_chord(degree, None),
            Command::Generate { slot, settings } => self.generate(slot, settings),
            Command::FollowHarmony { slot, enabled } => self.follow_harmony(slot, enabled),
            Command::FreezeHarmony { slot } => self.freeze_harmony(slot),
            Command::Undo => self.undo(),
        }
    }
    fn remember(&mut self, event: InputEvent) {
        if self.recent.len() == self.limits.recorded_events
            && let Some(evicted) = self.recent.pop_front()
        {
            self.history_floor = self
                .history_floor
                .max(BeatTime(evicted.at.0.saturating_add(1)));
        }
        self.recent.push_back(event);
    }
    /// Copies recent input through an explicit host boundary without advancing
    /// live input time or releasing its notes. The cutoff must follow the last
    /// accepted input; only gestures already received enter the copied clip.
    pub fn capture_recent_at(
        &mut self,
        slot: usize,
        length: BeatTime,
        cutoff: BeatTime,
    ) -> Result<()> {
        if cutoff <= self.at {
            return Err(Error::TimeOrder);
        }
        self.capture_recent(slot, length, cutoff)
    }
    fn capture_recent(&mut self, slot: usize, length: BeatTime, cutoff: BeatTime) -> Result<()> {
        self.ensure_editable(slot)?;
        if length.0 == 0 {
            return Err(Error::Invalid("retrospective length"));
        }
        let start = BeatTime(
            cutoff
                .0
                .checked_sub(length.0)
                .ok_or(Error::Invalid("retrospective range before clock origin"))?,
        );
        if start < self.history_floor {
            return Err(Error::Capacity("retrospective history expired"));
        }
        let mut clip = crate::edit::retrospective_clip(
            self.recent.iter().copied(),
            self.session.slots[slot].track,
            start,
            cutoff,
        )?;
        clip.harmony_origin = self
            .recent_harmony
            .iter()
            .filter(|g| g.input.at >= start && g.input.at <= cutoff)
            .copied()
            .map(|mut g| {
                g.input.at.0 -= start.0;
                g
            })
            .collect();
        if clip.events.is_empty() {
            return Err(Error::State("no recent notes in requested range"));
        }
        self.commit_clip(slot, clip);
        Ok(())
    }
    fn begin_record(&mut self, slot: usize) -> Result<()> {
        if slot >= self.session.slots.len() {
            return Err(Error::NotFound);
        }
        if self.recording.is_some() || self.players.iter().any(|p| p.slot == slot) {
            return Err(Error::State("slot must be stopped and recorder idle"));
        }
        let pending = self
            .active
            .iter()
            .filter(|voice| {
                voice.pending_attack.is_some()
                    && voice.arpeggio.is_none()
                    && voice.track == self.session.slots[slot].track
            })
            .count();
        if 2 * pending > self.limits.recorded_events {
            return Err(Error::Capacity(
                "recording cannot reserve pending strum notes",
            ));
        }
        self.recording = Some(Recording {
            previous: None,
            slot,
            start: self.at,
            events: Vec::with_capacity(self.limits.recorded_events),
            held: Vec::with_capacity(self.limits.voices),
            harmony_origin: Vec::with_capacity(self.limits.recorded_events),
        });
        Ok(())
    }
    fn finish_record(&mut self) -> Result<()> {
        let recording = self
            .recording
            .as_ref()
            .ok_or(Error::State("not recording"))?;
        let length = BeatTime(self.at.0 - recording.start.0);
        let clip = recording.clip_at(self.at, self.session.slots[recording.slot].track)?;
        let slot = recording.slot;
        let resume = if recording.previous.is_some() {
            let phase = length.0 % clip.length.0;
            Some(
                self.at
                    .checked_add(BeatTime((clip.length.0 - phase) % clip.length.0))?,
            )
        } else {
            None
        };
        self.stop_slot(slot)?;
        self.recording = None;
        self.commit_clip(slot, clip);
        if let Some(loop_start) = resume {
            self.players.push(Player {
                slot,
                loop_start,
                scheduled_start: loop_start,
                cursor: 0,
            });
        }
        Ok(())
    }
    fn begin_overdub(&mut self, slot: usize) -> Result<()> {
        if self.recording.is_some() {
            return Err(Error::State("recorder must be idle"));
        }
        let previous = self
            .session
            .slots
            .get(slot)
            .and_then(|s| s.clip.clone())
            .ok_or(Error::NotFound)?;
        if previous.follow.is_some() {
            return Err(Error::State("freeze harmonic following before overdub"));
        }
        if previous.events.is_empty() {
            return Err(Error::State("overdub requires a recorded clip"));
        }
        self.stop_slot(slot)?;
        self.begin_record(slot)?;
        if let Some(recording) = &mut self.recording {
            recording.previous = Some(previous);
        }
        self.players.push(Player {
            slot,
            loop_start: self.at,
            scheduled_start: self.at,
            cursor: 0,
        });
        Ok(())
    }
    fn ensure_editable(&self, slot: usize) -> Result<()> {
        if slot >= self.session.slots.len() {
            return Err(Error::NotFound);
        }
        if self.players.iter().any(|p| p.slot == slot)
            || self.recording.as_ref().is_some_and(|r| r.slot == slot)
        {
            return Err(Error::State("stop slot before editing"));
        }
        Ok(())
    }
    fn duplicate(&mut self, from: usize, to: usize) -> Result<()> {
        self.ensure_editable(to)?;
        let mut clip = self
            .session
            .slots
            .get(from)
            .and_then(|s| s.clip.clone())
            .ok_or(Error::NotFound)?;
        for event in &mut clip.events {
            event.note.track = self.session.slots[to].track;
        }
        for entry in &mut clip.conditions {
            entry.note.track = self.session.slots[to].track;
        }
        self.commit_clip(to, clip);
        Ok(())
    }
    fn edit_note(&mut self, slot: usize, note: NoteKey, edit: NoteEdit) -> Result<()> {
        self.ensure_editable(slot)?;
        let clip = self.session.slots[slot]
            .clip
            .as_ref()
            .ok_or(Error::NotFound)?
            .edit_note(note, edit)?;
        self.commit_clip(slot, clip);
        Ok(())
    }
    /// Publish a prepared control-thread edit and its complete Undo snapshot.
    fn commit_clip(&mut self, slot: usize, clip: Clip) {
        self.undo = Some(UndoState::Clip {
            slot,
            clip: self.session.slots[slot].clip.replace(clip).map(Box::new),
        });
    }
    fn set_note_condition(
        &mut self,
        slot: usize,
        note: NoteKey,
        condition: NoteCondition,
    ) -> Result<()> {
        self.ensure_editable(slot)?;
        let clip = self.session.slots[slot]
            .clip
            .as_ref()
            .ok_or(Error::NotFound)?
            .with_note_condition(note, condition)?;
        self.commit_clip(slot, clip);
        Ok(())
    }
    fn launch(&mut self, slot: usize, quantum: BeatTime) -> Result<()> {
        let clip = self
            .session
            .slots
            .get(slot)
            .and_then(|s| s.clip.as_ref())
            .ok_or(Error::NotFound)?;
        if clip.events.is_empty() {
            return Err(Error::State("clip has no notes"));
        }
        if self.recording.as_ref().is_some_and(|r| r.slot == slot) {
            return Err(Error::State("slot is recording"));
        }
        let delay = if quantum.0 == 0 {
            0
        } else {
            (quantum.0 - self.at.0 % quantum.0) % quantum.0
        };
        let loop_start = self.at.checked_add(BeatTime(delay))?;
        // Reject schedules whose first iteration could overflow before mutation.
        loop_start.checked_add(clip.length)?;
        self.stop_slot(slot)?;
        self.players.push(Player {
            slot,
            loop_start,
            scheduled_start: loop_start,
            cursor: 0,
        });
        Ok(())
    }
    fn stop_slot(&mut self, slot: usize) -> Result<()> {
        if slot >= self.session.slots.len() {
            return Err(Error::NotFound);
        }
        self.release_matching(|owner| matches!(owner, Owner::Clip(s, _) if s == slot))?;
        self.players.retain(|p| p.slot != slot);
        Ok(())
    }
    fn release_matching(&mut self, matches: impl Fn(Owner) -> bool) -> Result<()> {
        let count = self
            .active
            .iter()
            .filter(|v| matches(v.owner) && v.pending_attack.is_none())
            .count();
        if self.output.len() + count > self.limits.output_events {
            return Err(Error::Capacity("output; drain before stop"));
        }
        let mut index = 0;
        while index < self.active.len() {
            if matches(self.active[index].owner) {
                let voice = self.active.remove(index);
                if voice.pending_attack.is_some() {
                    continue;
                }
                if matches!(voice.owner, Owner::Live(_) | Owner::Harmony(_)) {
                    let note = voice.rendered_note;
                    self.remember(InputEvent {
                        at: self.at,
                        note,
                        gesture: Gesture::Off { velocity: 0 },
                    });
                }
                self.output.push_back(OutputEvent {
                    at: self.at,
                    destination: voice.destination,
                    track: voice.track,
                    voice: voice.id,
                    gesture: Gesture::Off { velocity: 0 },
                });
            } else {
                index += 1;
            }
        }
        Ok(())
    }
    fn stop(&mut self) -> Result<()> {
        self.release_matching(|_| true)?;
        self.fill = false;
        self.players.clear();
        // Closing the take is explicit: stop cancels the uncommitted take and
        // leaves the previous committed clip intact.
        self.recording = None;
        Ok(())
    }
    fn undo(&mut self) -> Result<()> {
        if self.recording.is_some() {
            return Err(Error::State("recorder must be idle"));
        }
        if let Some(UndoState::Clip { slot, .. }) = &self.undo {
            self.stop_slot(*slot)?;
        }
        if let Some(UndoState::LearnedChord { degree, chord }) = &self.undo {
            self.harmony_with_learned_chord(*degree, *chord)?;
        }
        match self.undo.take().ok_or(Error::State("nothing to undo"))? {
            UndoState::Clip { slot, clip } => {
                self.session.slots[slot].clip = clip.map(|clip| *clip)
            }
            UndoState::LearnedChord { degree, chord } => {
                self.session.harmony.learned[usize::from(degree)] = chord
            }
        }
        Ok(())
    }
    /// Device disconnect releases only this source's live voices; clip playback
    /// remains independent. Any in-progress take is cancelled for safe recovery.
    pub fn disconnect(&mut self, source: SourceId) -> Result<()> {
        self.release_matching(
            |owner| matches!(owner, Owner::Live(key) | Owner::Harmony(key) if key.source == source),
        )?;
        self.fill = false;
        self.recording = None;
        Ok(())
    }
}
fn release_order(gesture: Gesture) -> u8 {
    if matches!(gesture, Gesture::Off { .. }) {
        0
    } else {
        1
    }
}
