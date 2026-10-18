# Crest Performance

An independent Rust performance library and firmware-backed LinnStrument development host.
The workspace is named `crest-performance`; it currently lives in the existing
`crest-linnstrument` repository. No Crest Tracker checkout, application code, sound engine,
physical controller, MIDI device, or sound card is needed for the development path.

**Released v0.4.0: API 13, session 10, host envelope 2.**
**Development checkout: API 14, session 11, host envelope 2.**

Dedicated Chords uses firmware User Mode with one I–VII octave bank on the right,
compact quality/extension and voicing rows on the left, following Crest Flower’s
control grouping. Diatonic/Chromatic and articulation controls sit directly below.
Root and scale stay above the instrument. Unassigned pads are dark. Strumplate has
its own harmony selector and tone pads. Custom surfaces preserve per-cell expression;
crossing cells retriggers. Stock Play retains firmware MPE slides.

The expanded palette includes major/minor/diminished/augmented qualities, sixths,
add9 without a seventh, seventh/ninth variants, suspensions and borrowed harmony.
These changes are unreleased; Crest Synth/Tracker consumers keep their existing pins.

The [HTML user manual](docs/manual.html) opens with a virtual LinnStrument reference:
select Play, Chords, Strumplate or Clips, then hover or focus pads to read their actions
and decoded firmware light states. Four captioned surface demos cover the development UI;
11 complete v0.4.0 workflow demos remain in expandable reference chapters. All videos,
images and captions are embedded for offline use. The guide distinguishes released
musical features from the unreleased surface redesign and chord palette.
Edit `docs/manual-source.html` and run `python3 scripts/build-manual.py` to rebuild the offline HTML.

The development simulator opens the instrument only. Its compact mode buttons select
Play, Chords, Strumplate or Clips. Hover identifies notes and control actions; focus the
surface and use Alt+arrow keys to inspect without playing. **Tools** opens the existing
standalone host for recording, editing and session work; closing it returns to the
instrument. **Input / device options** reveals diagnostics. Closing the instrument exits
the application. MIDI and firmware behavior remain behind the same public library and
worker boundaries. Crest Synth remains the eventual production sound engine; the
standalone capture backend does not generate audio.

The previous `v0.3.0` release uses API 12 and the same persistence formats.
The firmware-backed play/record/loop/pad-launch path runs without Crest Tracker or hardware.
The matching [v0.4.0 release tag](https://github.com/crestenstclair/crest-linnstrument/releases/tag/v0.4.0)
defines this contract; an untagged checkout remains a candidate. See
[qualification evidence and limits](docs/status.md).

Version 0.4 adds temporary clip octaves and firmware-pad articulation selection.
Version 0.3 added firmware-pad harmony controls, local tempo changes, explicit attack
quantization, A/B morph capture and voice-scoped parameter locks. Version 0.2 added timed strumming, humanized chords, learned chord banks, repeating arpeggios,
per-note conditions, chord-clip leaders and temporary mute. The first integration baseline,
[v0.1.0](https://github.com/crestenstclair/crest-linnstrument/releases/tag/v0.1.0), remains
API 2/session 2. Crest Tracker's independent integration branch now consumes released v0.3.0 commit
`a05f8df` (API 12). It remains pinned while this repository advances API 14.
Schemas 1–10 migrate to schema 11. Saving with this development checkout writes schema 11,
which released v0.4.0 and older readers reject. Keep original files when testing upgrades.
The released v0.3 and v0.4 packages continue to share schema 10.

API 13 adds temporary clip transposition. **Hold octave down/up** changes
only future attacks of the selected playing clip; stored notes, held expression and releases
remain intact. In **Clip pads**, rows 3 and 4 above bottom row 0 hold down/up for each clip
column. Blue means available; magenta shows the effective shift. Host and firmware contacts
retain independent ownership. Opposing directions cancel, then the remaining direction
resumes when one releases. Focus loss releases host controls and momentary virtual contacts;
latched fingers require explicit release. Pitch overflow rejects without clipping notes.
These controls are introduced in v0.4 and are not part of the v0.3 tag.

The test host's **A/B MORPH** panel configures up to 16 parameter endpoints. Add parameters,
choose destination/reference and normalized A/B values (0–65535), then Apply. Buttons select
exact endpoints; the slider interpolates all values together. The explicit parameter capture
sink displays the resulting timestamped events. These references are not MIDI CC numbers;
there is no implied DSP mapping or audible effect. A production host must advertise and map
its own supported parameters.

With a scene configured, **Clip pads** reserves the third row from the bottom for continuous
morphing. Touch and drag across that row: pinned stock firmware generates 14-bit X data and
slide handovers through the normal sensor path. The first finger owns the row until release;
other held fingers cannot steal it. Blue marks the row, and decoded firmware green shows the
current position. A/B endpoints persist; the current gesture position and backend capabilities
do not. Reopening emits no parameter values. Notes, expression and clips remain unchanged.
Parameter automation recording remains further work.

The note editor adds voice-scoped parameter locks to recorded or generated notes. Choose
a parameter reference 0–15 and normalized value 0–65535, then Apply. Remove clears that
reference only; Undo restores the complete previous clip. Values follow each note through
moves, quantization, duplication, save/reopen and loop playback. Conditions and temporary
mute suppress both the attack and its locks. Each iteration binds values to a fresh voice;
other notes keep their instrument defaults. The A/B panel capture trace distinguishes
global values from voice IDs. This is explicit capture-backend support, with no MIDI CC
conversion or production sound-engine mapping.

Conditional playback keeps the original notes and expression. In the note editor, select a
note with the canvas or Previous/Next, then choose every loop, loop N of a period, Fill or
Not Fill. Rules decide at attack only; held notes always retain their expression and release.
Hold the host's **Fill** button with the pointer, or focus it and hold Space/Enter. Focus
loss releases Fill. Edits require a stopped slot and support Undo; each launch restarts the
loop count. Fill is temporary and is never saved in a session.

**Humanized chord** adds repeatable timing and velocity variation to held chords. Set maximum
delay, velocity variation and seed in Harmony. Each trigger gets its own deterministic
variation. Expression follows each voice from its actual attack; releasing early cancels
waiting tones. Bass and manual strumplate remain immediate. Recording stores exact rendered
notes plus the original gestures and settings, so replay does not roll the dice again.

Choose **Follower harmony source** in Harmony to follow a recorded chord clip. Its original
chord gestures lead accompaniment while it loops, even when its rendered notes are strummed
or humanized. Followers keep rhythm, expression and held-note pitches. Stopping the leader
restores live/selected harmony. The choice survives save/reopen; reopening starts stopped.

**Hold mute selected** silences only the selected running/queued clip. Its loop keeps
advancing, and recorded harmony still leads accompaniment. Release restores future
attacks without reviving already-passed notes. The virtual pad shows cyan while muted.
Mute is temporary, clears on safety transitions, and never changes saved clips or Undo.

## Run

Install Rust 1.96.0, Python 3, Git, and a C++17 compiler. The desktop uses Tauri 2.12.4,
tauri-build 2.7.3, and plain HTML/CSS/JavaScript, confirmed against current Crest Tracker's
GitHub lockfile and frontend. These are ordinary framework dependencies; no Tracker code
was copied or forked.

```sh
git submodule update --init --recursive
cargo test --workspace --locked
scripts/build-native.sh
python3 scripts/test-firmware.py
cargo test -p linnstrument-sim --locked --lib -- --include-ignored
cargo run -p linnstrument-sim --features desktop --locked
```

On Linux, the desktop additionally needs WebKitGTK 4.1 and GTK 3 development packages:

```sh
sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev patchelf libasound2-dev
```

The desktop starts with the instrument. Select **Tools** to open the standalone host.
Select a host slot, choose **Record selected**, play the instrument, then **Finish take**.
Click the selected recorded slot to launch. **Clip pads** switches stock firmware into
User Firmware Mode: bottom-row pads 1–8 launch or stop slots. **Play mode** restores stock
MPE playing while loops continue. Save and reopen through the host's session path field.
**Launch grid** selects immediate, next-beat or next-four-beat launches for both host buttons
and controller pads. Queued pads show yellow, playing pads green. A second press cancels a
queued launch or stops playback immediately. This live host preference applies to future
launches, survives device reset/session open, and starts at Off on application restart.

**Local tempo** changes the standalone host clock from 1–999 BPM, with 0.001 BPM
resolution. Apply it while playing or recording: held expression and clip phase continue,
and queued launches keep their musical boundaries. Recording preserves beat positions
under the active tempo; this does not add a recorded tempo map. Save/reopen and firmware
reset retain the current tempo. External MIDI mode uses the connected host's clock.
New host files use envelope 2; envelope-1 and plain sessions load at 120 BPM without
rewriting the source. Stable v0.2 rejects envelope 2, so keep original files for rollback.
The embedded core session uses schema 10 and can still be consumed independently.

**Clip pads** also uses top-row pads 1–7 for I–VII.
Pressing one selects that degree using the configured harmony settings and takes over
follower harmony from any recorded leader. The active effective degree is green; the
other selectors are blue. A running chord source moves that indicator until a degree is
pressed. Held notes and fixed clips keep their pitches; future follower attacks use the
selection. The selection persists with the session. All pad input still crosses the
firmware sensor path, and the displayed LEDs still come from firmware output.

Beside Fill, row 6 columns 2–7 step inversion down/up, register down/up, toggle spread,
and toggle voice leading. They take over the current effective chord from a recorded
leader, preserving its degree and other settings. Held voices and fixed clips stay
unchanged; future chord/follower attacks use the new voicing. Blue pads are available;
enabled toggles are green. Invalid steps are dark and inert, including MIDI pitch limits.
Release before pressing again; duplicate attacks do not repeat steps. These changes
persist with harmony settings. The host displays current inversion, register and toggles.

Row 5, columns 1–5, toggles seventh, ninth, sus2, sus4 and parallel-scale borrowing.
Extensions and suspensions each select one option; pressing the active option again
restores triad or no suspension. Borrowing combines with either. These use the same
takeover, release-before-repeat and blue/green/dark feedback as voicing controls. Learned
chord entries retain fixed pitches, so their diatonic modifiers are dark and inert.
Return to **Chord pads** to play the selected settings through stock MPE.

Row 5 columns 6–11 selects Block, ascending/descending strum, ascending/descending
arpeggio and Humanized. The selected articulation is green; alternatives are blue.
Direction changes retain custom spacing and gate. Entering another family uses 1/16-beat
steps, a 1/32-beat arpeggio gate, or Humanized with a 1/16-beat maximum delay, velocity
variation 8 and seed 42. Reselecting the current family retains custom values. These
controls work with learned chords and take over selected harmony. Held voices, existing
clips and loop phase keep their original timing. Return to Chord pads to play the selection.

Hold the pad directly above a running/queued clip to mute it; both pads turn cyan.
Release resumes future attacks at the existing loop phase. Dark mute pads are inactive.
The pad directly below degree I holds **Fill**, yellow when ready and magenta while held.
Independent fingers can hold Fill and several clip mutes concurrently. Host controls and
controller contacts combine: releasing one leaves the other active. Stop, mode changes,
disconnect and replacement players retire their old hold ownership. Simulator focus loss
releases momentary contacts; visibly latched contacts continue until released explicitly.

For the expansion demo, enter the absolute path to `examples/performance-showcase.json`
in the host's session field and choose **Reopen**. The first three slots contain:

- **01 — expressive chords:** the bundled firmware-captured phrase rendered through seeded
  humanization, retaining its free timing, expression and generating gestures.
- **02 — following bass:** three Euclidean pulses over two beats, routed to destination 2.
- **03 — Fill accompaniment:** four seeded notes per beat, routed to destination 3 and
  silent until the host's **Fill** control is held.

Select each slot, then click it again to launch; all three loop independently. In **Clip
pads**, the first three bottom-row pads launch/stop them with firmware LED feedback.
The bass and Fill parts follow the selected/live harmony on new attacks; the recorded
chord phrase retains its pitches. Return to **Chord pads** and hold one chord trigger
while the bank runs to explore the default 15-voice budget. These destinations use the
mandatory MIDI capture sink; optional reference audio is not connected.

The reproducible builder uses only public APIs and checks destination isolation, balanced
MPE ownership, Fill silence and exact save/reopen:

```sh
cargo run -p crest-performance-host --example performance_showcase -- target/performance-showcase.json
```

This builder replays an existing firmware capture through the library. Live sensor/firmware
acceptance remains the separate native and desktop test suites.

**Chord pads** keeps stock MPE input and renders diatonic chords in the Rust library.
The Harmony panel selects scale, sevenths/ninths, suspensions, borrowed harmony, inversion,
spread, register, voice leading and all-voices/top-voice expression. Seven successive
input pitches choose I–VII; the stock input anchor is MIDI 30. Strumplate maps new pad
attacks to selected chord tones across octaves. Slides retain stock firmware note ownership
and bend the held note; they do not synthesize extra harp attacks. Held voices keep their
original pitches when controls change. Record/save/reopen preserves their rendered curves
and source gestures. A separate bass track can be chosen from tracks present in a session.
**Articulation** selects timed strums or repeating arpeggios, low-to-high or high-to-low.
Step spacing sets the beat interval; an arpeggio gate sets each note's duration, up to that
interval. A held finger repeats its original voiced chord, with independent X/Y/Z applied
to sounding and future notes under the selected expression policy. Each repeat has a new
note identity. Release cancels future attacks and releases sounding notes. Bass remains
held; Strumplate remains manually picked. Record stores exact gates and expression.
To learn a voicing, use Play mode, latch 1–5 notes, select a degree in the host and click
**Learn held notes**. The bank preserves fixed MIDI pitches and unisons; it ignores bends
when learning. An optional root supports rootless voicings and routed bass. Release fingers,
then select Chord pads to play the learned voicing from one pad. Inversion, spread, register,
voice leading, strum and expression still apply. Empty entries use diatonic chords.
**Restore selected degree to diatonic** clears one entry; both bank edits support Undo.

Release the trigger to cancel remaining tones. Expression updates sounding voices and the
initial state of upcoming tones, respecting the selected all/top-voice policy. Bass sounds
immediately; manually picked Strumplate notes retain immediate attacks. Capture stores the
rendered timing, expression and source gestures for exact replay without regeneration.

Expand **Accompaniment** to generate a selected stopped slot. Euclidean pulses, seed,
pitch-cycle length, chord-tone range and bounded timing/velocity variation are independent
controls. Generated clips contain the exact rendered performance and algorithm/settings
metadata. Enable **Follow harmony** for future attacks to follow selected/live chord changes.
**Freeze current harmony** stores fixed pitches; Undo restores the previous clip. Different
slots loop independently. Stop a slot before generating, freezing, or changing follow mode.
Freeze a following clip before overdubbing.

The **Output** panel reports destination capabilities and configures MPE, ordinary MIDI
polyphony or explicit monophonic reduction. Stop transport first. Ordinary output can
reject unsupported pitch/timbre or deliberately keep only per-key pressure. Mono uses
last-held-note priority. Host sessions retain profiles; plain library sessions remain
readable with lower-MPE defaults. Independent expressive playback requires MPE. These are initial M4/M5 slices, not completion
of the full harmony/generator backlog.

The note editor offers **Quantize clip attacks** as an explicit stopped-clip
edit. It chooses the nearest feasible beat-grid position for each attack, with halfway
ties moving later. The whole note moves: duration, velocity and all X/Y/Z samples stay
exact. Near a loop boundary, a note moves earlier if necessary to fit its complete curve.
Undo restores the exact original performance, and recording remains unquantized. The
library exposes the same operation through `Command::QuantizeClip`, introduced in API 10.
Quantization only moves raw events; current saved sessions still require a schema-10 reader.

**Play scripted phrase** drives three independently expressive contacts through the native
scheduler. Start host recording first to capture it, then finish and launch the clip. The
script runs without browser timing and Release All cancels it. Custom version-1 contact
scripts can be loaded from the capture panel; see
[`expressive.json`](crates/linnstrument-sim/scenarios/expressive.json) for the format.
The saved [`expressive-phrase.json`](examples/expressive-phrase.json) is another ready-to-open
example, captured from the pinned firmware through the synthetic sensor path.

Mouse dragging moves a contact. Wheel, brackets, or the pressure slider adjust Z. A–K
shortcuts hold notes; arrows move the selected contact. Shift uses fine movement. Latch
keeps contacts held; click a contact button to select it and double-click to release it.
Expand **Keyboard contacts** to assign letter/digit keys to sensor coordinates and choose
pressure keys. Settings validate against geometry and persist in this simulator. OS key
repeat never retriggers attacks; release ownership uses physical key codes, so modifier
changes cannot strand a held note. Text/number editors and OS shortcuts do not play notes.
Expand **Mouse / keyboard pressure profile** for linear attack/release times from 0–2 seconds.
The native clock samples pressure every 1 ms for mouse, keyboard and scripted contacts.
Zero uses immediate pressure. At 16 fingers, a newly pressed finger retires the oldest
released tail; active fingers are never stolen. Focus loss or lost pointer capture releases
momentary contacts immediately. Release All cancels scripts and clears every contact.
Scripts retain ownership through their release tails and survive window focus changes.
Contact rings
are an optional overlay; illuminated pad colors come only from decoded firmware SPI writes.
The control column is part of the sensor surface, so its settings UI is the firmware's UI.

Set `LINNSTRUMENT_MODEL=128` for the smaller instrument. `LINNSTRUMENT_WORKER` and
`LINNSTRUMENT_FLASH` override development asset paths. Packaged candidate assets can sit
beside the executable. A failed worker disconnects the replica; there is no mock fallback.
No reference audio backend is currently connected. The two pedal buttons operate emulated
foot-switch pins; the right pedal defaults to sustain.

Expand **Capture / replay gesture run** to export a manual run. Release contacts, stop the
host and pause, then verify it. Replay starts another firmware process and compares exact
MIDI, LED frames, timestamps and peripheral counters. It does not modify the active session.
The bounded run buffer starts at boot/reset; reset preserves committed clips.

```sh
cargo run -p linnstrument-sim --bin linnstrument-replay --locked -- \
  /tmp/linnstrument-run.json target/firmware-native/linnstrument-worker \
  target/firmware-native/ideal-200.bin
```

## OS MIDI devices and external hosts

The optional `midi` feature uses midir 0.11.1 (CoreMIDI on macOS, ALSA on Linux). Default
headless/core builds do not load an OS MIDI backend. To expose the firmware replica to
other applications instead of its embedded development host:

```sh
LINNSTRUMENT_VIRTUAL_MIDI=1 cargo run -p linnstrument-sim --features desktop --locked
cargo run -p crest-performance-host --features midi --locked -- ports
cargo run -p crest-performance-host --features midi --locked -- midi INPUT_ID - FEEDBACK_OUTPUT_ID
```

Use IDs printed by `ports`: **LinnStrument Simulator performance** is host input and
**LinnStrument Simulator feedback** is controller output. Replace `-` with a distinct sound
output port ID to audition an MPE instrument. Physical devices use the same CLI and adapter;
configure lower MPE, channels 2–16 and ±48 semitones first. No automatic MIDI thru is added.
The CLI accepts `record 1`, `finish`, `launch 1`, `clips`, `play`, `stop`, `save PATH`,
`open PATH`, `chords`, `harmony JSON`, `profile JSON`, `quantum TICKS`, `tempo MILLIBPM`, and `quit`. Use
`quantum 960000` for next-beat launches or `quantum 0` for immediate launches.
Use `tempo 90500` for 90.5 BPM; this is the local host clock, not external MIDI clock sync.
Ctrl-C/termination, EOF, overflow and detected removal attempt
voice cleanup. Reconnect by reopening the current port IDs. OS round-trip tests ran on
CoreMIDI; Linux runtime and physical USB qualification remain separate.

## Independent headless library

These commands do not build the simulator, require a C++ compiler, or fetch firmware:

```sh
cargo build -p crest-performance-core -p crest-linnstrument --release --locked
cargo run -p crest-performance-host --locked -- demo
cargo run -p crest-performance-host --locked -- inspect target/demo-session.json
cargo run -p crest-performance-host --example processing_bench --release --locked
```

The CLI `demo` is explicitly a **protocol fixture**, useful for library regression checks.
The firmware-backed demonstration is the ignored-by-default native integration test above;
CI builds its worker and runs it explicitly. It records three expressive sensor contacts,
reopens the session, launches through a user-mode sensor pad, and verifies firmware LED output.

The processing benchmark measures three virtual minutes of six live and six looping notes
with full expression at 100 Hz, including overlapping pitches and MPE allocation. It reports
p50/p95/p99/max batch cost and verifies balanced releases. Sensing, OS MIDI, UI, file I/O and
audio are excluded from timing. CI publishes observations without treating a shared runner
as a Steam Deck latency qualification.

- `crest-performance-core`: host-clock engine, stable note/voice identities, clip streams,
  recording, overdub, retrospective capture, launch quantization, editing, duplication, undo.
- `crest-linnstrument`: byte parser, MPE input/output allocation, user-mode coordinates, LED diffs.
- `crest-performance-host`: public-API orchestration, capture sink, atomic session files, CLI.
- `crest-performance-testkit`: deterministic protocol fixtures and behavioral contract tests.
- `linnstrument-sim`: typed native bridge, Tauri views, separate pinned C++ firmware worker.

See [public contract](docs/contract.md), [firmware build and fidelity](docs/firmware.md),
and [delivery status](docs/status.md). The original implementation plan remains the roadmap.

## CI and candidates

Independent CI checks Rust without the firmware checkout, native firmware on Linux/macOS,
the original ARM build/layout, and Linux desktop mouse/keyboard record/reopen/launch tests.
`candidate-artifacts` creates relocatable development archives with checksums, provenance
and dependency notices, then exercises the extracted application. See [package usage](docs/package.md).
Tags and releases are published only after the matching source and extracted-package gates pass.
Crest Tracker consumes the accepted `v0.3.0` commit (`a05f8df`, API 12) in its own
repository. The API-13 library remains independent; upgrades are deliberate
integration changes validated by shared fixtures.

Licensed Apache-2.0. Upstream firmware is copyright Roger Linn Design, Roger Linn and
Geert Bevin, with contributions credited upstream. Its unmodified source, Apache license,
and bundled DueFlashStorage MIT attribution are retained in the pinned submodule.
