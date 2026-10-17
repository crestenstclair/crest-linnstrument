# Delivery status — October 16, 2026, Pacific

Version `0.1.0` is the first standalone integration baseline: API 2, session schema 2,
host document envelope 1. Acceptance is attached to the matching `v0.1.0` tag and release
checks; an untagged checkout is a candidate. There is no dependency on Crest Tracker and
no Tracker application code was copied. Its current framework versions were confirmed
from GitHub and the working checkout: Tauri 2.12.4, tauri-build 2.7.3, Wry 0.57.0, with
plain HTML/CSS/JavaScript.

## Standalone qualification

| Boundary | Evidence |
| --- | --- |
| Independent library | Core/adapter builds and tests without C++, firmware, GUI, audio, MIDI hardware or Tracker. A separate Cargo application consumed the Git-pinned public contracts. |
| Firmware | Unmodified pinned ARM baseline builds; native worker executes upstream setup/loop, sensors, touch processing, MIDI and LEDs. 119 ARM layout assertions; Linux/macOS normal and UBSan suites. |
| Raw MPE | Both geometries pass independent and concurrent three-finger X/Y/Z, repeated pitches, vibrato, slides, member capacity/reuse, sustain, timer wrap, user-mode return and LED round trips. Split lower/upper zones and ±24-semitone negotiation are independently checked. |
| Public library | Stable note ownership, live-plus-clip output allocation, destination profiles, queue rejection/cleanup, parser recovery, sustain, expression, loop boundaries and version rejection have behavioral tests. |
| Recording | Polyphonic expression survives recording, save/reopen, loop playback alongside live notes, basic editing, retrospective capture, duplication, overdub and undo. |
| Virtual pads | Firmware sensor contacts launch/stop clips. Quantized launches stay silent and show decoded yellow LEDs, start exactly on the musical boundary and show green LEDs. Both geometries pass. |
| Desktop | Linux Xvfb runs real X11 keyboard and native WebDriver mouse input through the webview/bridge/firmware. Three-finger expression, record/save/reopen, pad launch/stop, actual canvas LED pixels and focus cleanup pass. |
| Recovery | Worker failure, bounded bridge/input/output queues and a dropped release during overflow fail closed. Reset clears stale gestures and preserves committed clips. Pressure tails, scripts and focus/pointer recovery have native tests. |
| Stress | Eight clips with 16,016 stored events serialize losslessly below the host's 8 MB file bound. Over 90,000 scheduled events survive more than 1,000 output-capacity retries with exact times/curves and fresh voice identities. 128 successive overdubs preserve timing, expression and complete undo. |
| Package | Relocatable Linux x86_64 archive includes worker, both flash fixtures, examples, notices, provenance and checksums. The extracted package passes the same desktop scenario while build-tree firmware assets are hidden. |

The successful pre-release [desktop CI run 38998143577](https://github.com/crestenstclair/crest-linnstrument/actions/runs/38998143577)
includes Rust, desktop runtime, unmodified ARM and both native firmware platforms.
[Run 38998319513](https://github.com/crestenstclair/crest-linnstrument/actions/runs/38998319513)
verified the extracted Linux package. Subsequent release checks rerun these paths against
the release revision. Screenshots, session files, observations and driver logs are CI
artifacts, alongside independent firmware expectations in the tests. Captured traces do
not approve themselves as golden truth.

Local macOS desktop testing also exercised mouse/keyboard contact movement, independent
latched fingers, note selection/editing, harmony recording, pattern freeze, session reopening
and pad launch/stop. A 10,771-step pressure-envelope gesture replay matched exact firmware
MIDI/LED snapshots. The native clock samples optional 0–2-second pressure envelopes every
1 ms; browser frames only render snapshots. Contact overlays are distinct from firmware LEDs.

The processing benchmark covers three virtual minutes of six live and six looping notes,
full expression at 100 Hz, overlapping pitches and retrospective-history rollover. It
validated 216,546 events, 651,276 MIDI messages and 546 balanced attacks/releases. Observed
Linux x86_64 CI batch cost: p50 641 ns, p95 671 ns, p99 1,142 ns, max 121,767 ns; no batch
reached 1 ms. This measures library processing/allocation/capture validation, excluding
sensing, OS MIDI, GUI, files and audio. These observations are not Deck latency qualification.

## Explicit limits

- The included fixture and test host use lower MPE, main channel 1, members 2–16, ±48
  semitones. The adapter supports ordinary input and upper zones; selecting additional host
  input profiles and automatic device discovery remain expansion work. Split tests validate
  firmware generation separately. The test host's total engine voice budget is 15.
- Output destinations support MPE, ordinary polyphony or explicit monophonic reduction.
  Ordinary output cannot preserve independent pitch/timbre and rejects overlapping identical
  pitches; its discard policy is explicit. Profiles persist in host documents.
- Native compilation is source-level simulation, not execution of the ARM binary. Three
  portability patches and synthetic sensor assumptions are documented in [firmware notes](firmware.md).
  Physical calibration, sensor noise/latency, edge behavior and full ARM/native trace
  equivalence are unqualified. All upstream settings and split combinations have not been
  exhaustively tested. The control column uses actual firmware settings logic.
- Optional OS MIDI is implemented with midir. CoreMIDI port round trips and firmware through
  OS MIDI/host/return LEDs passed on macOS. Linux compiles and the packaged internal path runs;
  Linux OS-port runtime, physical USB and reconnect fidelity remain unqualified.
- No reference audio backend is connected. The mandatory capture sink proves musical events,
  not audible Crest Synth MPE. There is no new production synth or plugin host.
- Linux packaging was exercised on Ubuntu 24.04 with system GTK/WebKitGTK/ALSA dependencies.
  SteamOS packaging, controller navigation, audio, suspend/resume and physical hardware are M7.
  Virtual desktop workflow is qualified for the tested profiles; Deck and physical workflows
  are not qualified.

## Further library work and integration

Initial M4/M5 features are included: diatonic chord pads, extensions/suspensions, parallel-scale
borrowing, inversion/spread/register, bounded voice leading, bass routing, all/top-voice
expression, selected-chord strumplate, seeded Euclidean accompaniment, independent pitch
cycles, bounded humanization, live/selected harmony following and freeze. Rendered performance,
source gestures, settings, seed and algorithm versions persist. Full M4/M5 acceptance remains
open: repeating arpeggio articulation, chord-clip leaders, conditions,
parameter locks, momentary transforms and scene morphing remain future work.

M6 starts in Crest Tracker's own repository against the accepted M3 tag. Its native backend
is Rust, so a direct dependency is available; no speculative FFI wrapper was added. Tracker
owns its adapter, engine capability changes, production UI, project mapping and later Deck
package. M4/M5 can continue here while Tracker remains pinned to its accepted version.

## Post-baseline timed strum candidate

Development 0.2.0-dev now uses API 4/session 4; Tracker continues to consume its separately
pinned API-2 baseline. Timed ascending/descending strums retain trigger ownership, cancel
unplayed tones on release, apply independent expression and capture exact rendered timing.
Core tests cover queue pressure, recording reservations, stop/disconnect, top-voice policy,
bass, manual strumplate, equal-time ordering, overflow, recovery snapshots and migration.
The pinned firmware's three-finger phrase produces three raw attacks and nine precisely
spaced library notes on both 128- and 200-pad models, then survives save/replay. This is
native simulation evidence, not physical-device qualification or a new tagged release.

The macOS desktop candidate was exercised through native UI controls: select descending
1/16-beat strum, record the firmware's three-finger phrase, finish, save and reopen. The
saved schema-3 session contains nine attacks exactly 60,000 ticks apart within each chord,
36 expression events and 20 original gestures. The Linux desktop CI also exercises strum
controls, mouse input, exact rendered spacing and session reopening.

On this macOS/aarch64 development machine, the release processing benchmark ran three
repeating strum triggers (nine voices) alongside six looping notes, with expression at
100 Hz for three virtual minutes. Of 17,900 measured batches after warmup, p50 was 500 ns,
p95 667 ns, p99 1,334 ns and maximum 12,208 ns; none exceeded 1 ms. All 2,160 attacks had
matching releases. The timed work includes clock advancement, input, MPE allocation and
capture validation; sensing, UI, files, OS MIDI and audio are excluded. This is local
software-cost evidence, not total instrument latency or Steam Deck qualification.

Retrospective capture now also accepts an explicit host-clock cutoff without advancing live
input time. A firmware test captures three held contacts with independent pitch, timbre and
pressure, verifies no output/clock/ownership changes, then continues and releases the original
contacts. The standalone Capture button uses this boundary. Expired source-gesture history
cannot become available again when the rendered-event ring later evicts an older event.


## Post-baseline learned chord bank candidate

Seven pad degrees now accept learned live voicings. The existing voicing capacity is five
notes plus separate bass; unisons survive and overflow rejects the whole learn operation.
Learning is scoped to one source/track, leaves live ownership untouched and has Undo.
Bank entries use the existing voicing, articulation, expression, follower and generator paths.
Schema 4 loads schemas 1–3 losslessly and rejects falsely downgraded learned provenance.
Tracker's API-2 dependency remains unchanged.

Core tests cover fixed-pitch learning, source isolation, unisons, rootless bass intent,
transforms, pending strum ownership, live-plus-clip expression, Undo and malformed persistence.
The firmware scenario learns three sensor-generated notes, records the learned chord with
X/Y/Z expression, reopens its session and launches it from a firmware pad on both geometries.
Linux desktop acceptance includes three mouse-latched notes, Learn, expressive chord capture,
Undo and session reopening. These are virtual-development scenarios, not hardware qualification.
