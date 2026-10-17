# Public host contract, API 2

Rust consumers use the `crest-performance-core` and `crest-linnstrument` rlibs. No GUI,
filesystem, MIDI backend, audio backend, tracker document, or host scene type enters core.
The application owns one mutable Engine and its clock. The simulator's C++ worker is a
separate development dependency. No C ABI is introduced before a consumer needs it.

An application outside this workspace consumes the accepted baseline by tag:

```toml
[dependencies]
crest-performance-core = { git = "https://github.com/crestenstclair/crest-linnstrument", tag = "v0.1.0" }
crest-linnstrument = { git = "https://github.com/crestenstclair/crest-linnstrument", tag = "v0.1.0" }
```

A fresh external Cargo application verified engine creation, clock advance, session save,
snapshots and destination configuration against the development predecessor
`9ad6ea12ea56e3ea84f969c308aa64b47896dfd2`. Only the two libraries and Rust dependencies
compiled; no sibling checkout, GUI or C++ build was used. Cargo fetches the declared firmware
submodule when resolving the Git repository but does not compile it for these crates.
Keep the consumer's Cargo.lock and upgrade deliberately using the shared contract fixtures.

## Interrupted-take checkpoints

`Engine::checkpoint_session()` is an additive control-thread API after v0.1.0. It returns
an ordinary schema-2 session containing the captured prefix of an active take, with held
recorded notes released one tick after the current engine position. It does not advance
the engine, stop recording, emit or drain output, or change live voice ownership. Reopening
the snapshot starts stopped; the prefix is a committed clip that can be played or edited.
An empty take keeps the previous committed slot. Clock overflow and invalid snapshots are
reported without changing the engine. Normal `save_session()` still requires finishing or
canceling recording first. Hosts own checkpoint cadence, durable writes and recovery UI;
cloning and validation must stay outside audio callbacks. `checkpoint_session_at(cutoff)`
uses an explicit host-clock position after the last accepted input, retaining elapsed holds
even when the device sends no new packets. It never moves the input engine's clock.
Snapshot and FinishRecord share
the same clip-finalization rules, including overdub merging.

## Time and ownership

`BeatTime` is unsigned 64-bit musical ticks, 960,000 per quarter note. At 120 BPM a tick
is approximately 0.521 microseconds. No event is implicitly quantized. `ClockMapping`
converts host microseconds with integer arithmetic, an origin, and tempo in milli-BPM.
The host establishes a new mapping for tempo changes. Audio/sample conversion belongs
to the consuming host; neither browser frames nor this library sample a wall clock.

`NoteKey` contains source, track, and note identity. A source assigns a fresh ID on each
attack, including repeated pitches. `NoteInput` allocates monotonic IDs independent
of input channels. The engine assigns fresh output `VoiceId`s to live attacks and every
clip iteration. Each output destination has its own `MpeOutput` channel allocator.

Pitch expression is a signed offset in millionths of a semitone. Pressure and timbre
use unsigned 16-bit full range. Start events include initial expression. A stream
contains On, full expression samples, and Off; the samples are lossless authoritative
performance data. `Note` and `ExpressionCurve` are derived edit/view projections.

## Call order and errors

1. Construct `Engine::new(API_VERSION, session, limits)`. Version, session relationships,
   clip boundaries, and note lifecycles are validated before use.
2. Call `advance(timestamp)` before `input(event)` at that exact timestamp.
3. Drain scheduled output regularly. Playback is processed inclusively through target
   time. Equal-time live inputs preserve caller order after scheduled playback. Across
   players, releases precede new attacks. Within a recorded clip, original event order
   survives replay. Edits explicitly rebuild deterministic release-before-attack order.
4. Apply commands at the current engine position. Read snapshots and save on a control
   thread. Keep expensive clip edits, serialization, file I/O, and UI projection out of
   any eventual hard real-time callback.

Errors never silently replace a backend or a device. A full output queue leaves the
unaccepted input unchanged. `advance` may commit earlier events before hitting capacity;
it stops before the rejected event. Drain and retry the same target. Stop first checks
space for every release and can be retried after draining. The sample host fails closed
on processing/worker failures; it is a development scheduler, not an audio callback host.

`Limits` bound voices, output storage, recording, and retrospective history. Recording
reserves space for closing held notes. Retrospective history reports expiry rather than
pretending an incomplete take is complete. Continuous controls are not compressed.

## Clips and edits

One polyphonic clip stores an ordered raw gesture stream and explicit loop length.
Recording captures attacks beginning during the take; notes already held before record
are excluded. Finish truncates still-held recorded notes at the loop end without releasing
those live voices. Stop/cancel discards an uncommitted take, retaining the old clip.
Only releases may occur exactly at the loop boundary. Loop playback allocates new voices.

Overdub plays the existing loop while capturing a separate take. On commit, note positions
fold modulo the original loop length; crossing notes are explicitly truncated at that
loop end. Commit releases the old playback and queues the merged clip at the next boundary
relative to the overdub's origin. Finishing mid-cycle can therefore create a short gap.
The prior complete clip remains available to one-step Undo. Cancel keeps the original loop.

`TestHost::launch` applies the host's live `launch_quantum` preference to GUI, CLI and
user-mode controller launches. Zero is immediate. Nonzero schedules on the next multiple
of that many ticks, or immediately if already on the boundary. The scheduled timestamp
is fixed at launch; changing the preference never moves an already queued clip. Stop and
queued cancellation are immediate. The host retains this preference across device reset
and session replacement; it is not stored in the musical session or host document.

Duplicate retargets the copied clip's track. Editing requires a stopped slot. Moving a note
preserves expression offsets, including samples exactly at release. Repitching leaves
the original event sequence intact. Shortening explicitly trims points at or beyond its
new end; unedited notes retain every sample, including zero-duration notes. All
edits validate bounds before replacing a clip and retain the previous clip for Undo.
The small host piano roll performs selection, pitch, start, and duration edits in ticks.
There is no implicit tracker quantization. Arrangement, destructive quantization commands,
and a production tracker adapter are not yet implemented.

Changing Play/Chords/Clips mode releases live ownership and preserves running clips. It requires
finishing/cancelling any recording first. Device disconnect releases that source's live
notes; explicit host Stop releases all playback/live notes. Reconnect invalidates LED intent
and reestablishes configuration. The simulator reopens into stock Play mode.

## MIDI and surfaces

MIDI channels are zero-based in Rust and one-based in instrument-facing documentation.
`MpeZone` supports a contiguous standard upper or lower member zone, excluding main.
The standard fixture is lower MPE: main 1, members 2–16, bend range ±48 semitones,
CC74 for Y, Channel Pressure for Z. Configuration emits RPN zone and member bend ranges
with RPN nulling. Destination overflow rejects new voices, never steals existing ones.
Every attack resets sustain and supplies pitch/timbre/pressure before Note On. Every release
sends Note Off before expression reset. Out-of-range expression returns an error.

`NoteInput::accept` returns a borrowed batch, reused by the next call, with up to 128
active input notes including sustained tails. MPE assigns one held note to each member.
Ordinary polyphonic input supports channel pitch, CC74, channel pressure and key pressure.
Repeated same-channel pitches release FIFO because MIDI 1.0 does not carry note IDs.
Channel pressure supersedes prior key pressure on that channel; other channel controls
preserve per-key pressure. The standalone host defaults explicitly to the lower-MPE profile.

Sustain extends note durations in the event model. A released sustained note freezes its
last expression; a newly played note may reuse the input member but receives a fresh note
identity and a separate output voice. Main/member sustain flags combine; pedal release
emits releases in attack order. CC123 respects sustain; CC120 releases immediately. Output
allocation may reject capacity exhausted by sustained notes rather than stealing tails.

Incoming RPN 0 changes per-channel bend ranges (including cents); RPN 6 changes the selected
upper/lower zone's member count. A change affecting active notes is rejected. NRPN selection
cancels RPN selection. Zone disable requires a deliberate input-profile change. Main pitch
bend adds to member pitch, with domain bounds checked before output. The adapter does not
silently convert an arbitrary incompatible profile into the selected one. Explicit destination profiles expose capabilities and `require_mpe` validates voice count
and exact bend range. Automatic device capability discovery remains pending.

The parser uses fixed storage, supports fragmented bytes, running status, system common and
interleaved realtime, and skips unsupported SysEx without accumulating it. MPE member reuse
before release and mismatched release ownership are errors. The pinned firmware rotates
back onto an occupied member at its 16th touch without first sending an Off; the adapter
rejects this ambiguous input and the host fails closed, independently of output allocation. SysEx/device discovery support
and additional malformed-input fuzzing remain expansion work.

User Firmware Mode decodes rows from channels 1–8 and columns from note numbers. It is a
separate control path, never evidence of stock MPE generation. LED intent emits ordered
CC20/21/22 triples, clipped to geometry, with a caller budget. Intent is separate from MIDI
performance output. The worker's rendered RGB frames come from actual LED SPI/OE activity. Simulator mode
changes flush configuration and execute 100 ms of firmware before accepting another touch.
The OS CLI suppresses transitional incoming messages for 150 ms after changing modes.
These control transitions are explicit input gaps, not musical scheduling intervals.

## Persistence and compatibility

`Session.version == 2`; unknown schema versions are rejected. `Session::migrate` upgrades
schema 1 without changing tracks, slots, times or expression. Engine construction and the
host loader invoke it. Version-1 data carrying nondefault version-2 fields is rejected. The schema contains tracks,
instrument references and slots, not host documents or transient playback state. JSON uses
integer times and expression; save/reopen preserves them exactly. The host limits files to
8 MB, validates before replacement, and writes through a new sibling file plus sync/rename.
Saving an uncommitted take is rejected. Migration never overwrites the input file; explicit save writes schema 2.

The package is `0.1.0`, API 2, schema 2. No ABI version exists. Before 1.0, public or
persistence breaks require a deliberate version change and updated contract fixtures.
The accepted M3 tag `v0.1.0` defines the first integration baseline. Consumers should pin it;
a local path override is only a development convenience.

## Optional OS transport

The host's `midi` feature owns CoreMIDI/ALSA through midir. Callbacks copy at most 1,024 bytes
into a 256-packet queue without blocking. Overflow is sticky and stops input processing;
it never silently drops a release and continues playing. Callback arrival uses a host
`Instant` origin, separate from opaque driver timestamps. Late packets are applied at the
current host position; the original packet arrival remains available to callers.

The CLI dispatches on a native roughly 1 ms loop and polls endpoint existence once per
second. This is a development scheduler, not a sample-accurate audio backend or a Deck
latency qualification. Musical output precedes bounded LED batches. Input, sound output
and device-feedback routes are explicit, and sound/feedback cannot share an output port.
On disconnect/error/termination it attempts note releases, all-sound-off and normal-device
mode restoration. Software cannot guarantee delivery to an unplugged endpoint.

`output_since` returns captured messages after a monotonically increasing sequence count;
a consumer that falls behind the bounded history gets an error. The visible capture ring
is diagnostic history, not an unlimited recording. A successful send is never inferred
from a drawn LED or from firmware intent.

## Harmony and accompaniment (API 2)

`SetMode(Chords)` transforms stock MPE input in the deterministic core. It does not enable
User Firmware Mode. Source ID `u32::MAX` is reserved for rendered notes; external input
using it is rejected. Every generated voice has a fresh ID and a retained triggering
NoteKey. Repeated input pitches remain independent. Chord attacks, expression fan-out and
releases preflight the whole batch against voice/output/recording limits. No partial chord
is emitted on rejection. Recording captures rendered notes and lossless expression, plus
bounded `harmony_origin` source gestures, settings and algorithm version 1.

`HarmonySettings` defines tonic MIDI note, major/natural-minor/Dorian/Mixolydian scale,
degree (zero-based I–VII), triad/seventh/ninth, sus2/sus4, borrowed parallel scale, inversion,
open spread, octave register, voice leading, expression policy and optional bass track.
Borrowing uses natural minor for major/modal scales and major for natural minor. Negative
inversions move top voices down; positive inversions move bottom voices up. Spread raises
the second voice by an octave. Voice leading minimizes sorted voice movement over bounded
inversion/octave candidates, preserving requested voicing on equal costs. Invalid MIDI
pitches are rejected, never clipped. Bass is the uninverted root one octave lower.

Chord-pad input degree is `(input_key - input_anchor) mod 7`; default anchor 30 matches the
stock bottom-left pad. Strumplate maps input steps to successive voiced chord tones and
octaves. This is an alternative stock-MPE playing mode, not a claim of simultaneous custom
hardware regions. Continuous firmware slides retain ownership and per-note bends.
All-voices/top-voice expression policy is frozen at attack. Unselected voices start neutral
and ignore expression; every voice still receives its release. Settings changes affect
future attacks only. Mode change/disconnect releases generated voices as well as live ones.

`Generate` renders a bounded (1–128 step) Euclidean lane into a stopped slot, with Undo.
The algorithm uses modular evenly spaced pulses and rotation. Pitch cycle, chord-tone
range, seed, velocity and timing variation are independent settings. SplitMix64 recurrence
with fixed lane salts is pattern algorithm 1; changing density does not advance the pitch
random stream. Timing jitter is nonnegative and cannot extend a note beyond its step.
Velocity jitter clamps explicitly to MIDI 1–127. All generated pitches are validated before
replacement. `pattern_origin` retains seed/settings/harmonic intent; raw notes remain
playback truth. No generator runs during ordinary fixed-clip replay.

Following is explicit per clip. Each attack finds its nearest reference voiced chord tone
(ties choose the lower pitch), maps to the selected/live chord's corresponding tone index,
and retains the original chromatic offset. Times, releases, IDs and expression do not
change. Existing sounding notes never retune. `FreezeHarmony` renders current harmony into
fixed pitches and clears following, retaining historical provenance and one-step Undo.
Disabling following instead restores original raw pitches. Freeze before overdub; mixing
fixed incoming notes with relative playback is intentionally rejected. Editing preserves
historical provenance; it is not a claim that regeneration reproduces subsequent edits.

Clip state distinguishes initial queued launch from later loop positions, so silent loop
tails remain Playing. Generated lanes keep independent loop lengths and output ownership.
The host UI uses persistent clip buttons to retain focus and pointer activation while
snapshots update. No host framework or sound engine type enters these contracts.

## Destination profiles and host documents

`MidiOutput` wraps a validated `OutputProfile`. MPE keeps per-destination member ownership.
Ordinary polyphony uses one channel and MIDI Polyphonic Key Pressure. It rejects overlapping
identical pitches because their releases cannot be distinguished downstream. Its default
expression policy rejects pitch/timbre changes; `PressureOnly` explicitly discards those
unsupported dimensions. It never sends channel pitch or timbre while polyphonic notes sound.

Monophonic reduction uses last-held-note priority with a bounded 128-note stack. A new note
releases the previous sounding note before setting its expression and attacking. Releasing
the leader reattacks the previous held note with its latest expression. Updates/releases of
suppressed notes do not affect the sounding note. Cleanup releases the actual sounding note
without reattacking suppressed notes. This is explicit reduction, not independent MPE.

Profile setup clears incompatible MPE zone configuration before establishing the chosen
channel/zone and bend range. Each destination owns its MIDI port configuration; unrelated
routes must not share the same physical channel space. Output configuration is host control
work; musical event batches and ownership remain bounded. The test host exposes its separate
total engine voice budget (15), which can be lower than a backend's physical capacity.

`TestHost::configure_output` requires idle live voices, players and recorder. The host UI's
Output panel and CLI `profile JSON` apply these profiles. Host document version 1 wraps the
schema-2 library `session` plus per-destination `outputs`. Host save/open/reset preserve this
configuration. Every session destination needs exactly one validated profile; duplicate,
missing and unknown destinations fail before replacement. Legacy plain library sessions
remain readable and explicitly receive lower-MPE defaults. The independent core session
contains no MIDI backend types. Core `storage::save_session` remains available for fixtures;
the standalone application's Save uses `save_document`. Both retain atomic 8 MB-bounded I/O.
