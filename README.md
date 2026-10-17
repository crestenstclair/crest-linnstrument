# Crest Performance

An independent Rust performance library and firmware-backed LinnStrument development host.
The workspace is named `crest-performance`; it currently lives in the existing
`crest-linnstrument` repository. No Crest Tracker checkout, application code, sound engine,
physical controller, MIDI device, or sound card is needed for the development path.

**Standalone integration baseline: `0.1.0`, public API 2, session schema 2.**
The firmware-backed play/record/loop/pad-launch path runs without Crest Tracker or hardware.
The matching [release tag](https://github.com/crestenstclair/crest-linnstrument/releases/tag/v0.1.0)
establishes the accepted baseline; an untagged checkout remains a candidate. See
[qualification evidence and limits](docs/status.md).

Current development is **0.2.0-dev, API 5, session schema 5**. Timed strumming, learned chord banks and repeating arpeggios are
versioned library expansions; Crest Tracker remains pinned to its accepted API-2 commit.
Schemas 1–4 load losslessly; saving with this checkout writes schema 5, rejected by
older readers. Keep the original session to return to the baseline release.

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

The desktop opens two independent views: the instrument and the minimal standalone host.
Select a host slot, choose **Record selected**, play the instrument, then **Finish take**.
Click the selected recorded slot to launch. **Clip pads** switches stock firmware into
User Firmware Mode: bottom-row pads 1–8 launch or stop slots. **Play mode** restores stock
MPE playing while loops continue. Save and reopen through the host's session path field.
**Launch grid** selects immediate, next-beat or next-four-beat launches for both host buttons
and controller pads. Queued pads show yellow, playing pads green. A second press cancels a
queued launch or stops playback immediately. This live host preference applies to future
launches, survives device reset/session open, and starts at Off on application restart.

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
`open PATH`, `chords`, `harmony JSON`, `profile JSON`, `quantum TICKS`, and `quit`. Use
`quantum 960000` for next-beat launches or `quantum 0` for immediate launches.
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
It does not tag or publish an unqualified v0.1.
Crest Tracker will consume an accepted tag or commit in its own repository after M3.

Licensed Apache-2.0. Upstream firmware is copyright Roger Linn Design, Roger Linn and
Geert Bevin, with contributions credited upstream. Its unmodified source, Apache license,
and bundled DueFlashStorage MIT attribution are retained in the pinned submodule.
