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
open: live strum/arpeggio articulation, custom chord capture, chord-clip leaders, conditions,
parameter locks, momentary transforms and scene morphing remain future work.

M6 starts in Crest Tracker's own repository against the accepted M3 tag. Its native backend
is Rust, so a direct dependency is available; no speculative FFI wrapper was added. Tracker
owns its adapter, engine capability changes, production UI, project mapping and later Deck
package. M4/M5 can continue here while Tracker remains pinned to its accepted version.

The API-2 maintenance branch also fixes edited release ordering at zero-duration notes.
Shortening an earlier held note to that note's attack now frees the old voice first.
A shared public-contract fixture reproduces the original failure and verifies exact
Undo, session reopen and two loops through a one-voice engine/MPE destination. This
changes neither API 2 nor schema 2; the original v0.1.0 release tag remains unchanged.
