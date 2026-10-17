# Delivery status — October 16, 2026, Pacific

This checkout implements a standalone development baseline. It has no dependency on
Crest Tracker and no changes were made to Tracker. Current framework versions were
confirmed from GitHub (`crestenstclair/crest-tracker`, main Cargo.lock) and the working
checkout: Tauri 2.12.4, tauri-build 2.7.3, Wry 0.57.0; plain HTML/CSS/JavaScript.
There is no frontend npm framework package to fork or copy.

## Delivered and locally exercised

- Independent Rust workspace, public API/schema versions, bounded processing/storage,
  headless CLI, deterministic testkit, CI definitions and candidate artifact workflow.
- Pinned unmodified firmware, successful ARM build, reproducible native translation unit,
  C++ worker, peripheral services, ideal fixtures for both sizes and 119 ARM layout checks.
- Real firmware sensor-path MPE with independent three-contact X/Y/Z, continuous slides,
  wraparound, user-mode MIDI/LED round trips and deterministic replay.
- Separate Tauri instrument and host views. Actual desktop mouse and keyboard actions
  reached firmware, lit played cells, changed per-note expression and produced capture MIDI.
  A manually recorded desktop take was saved and reopened successfully. A real mouse
  selection/pitch edit changed exactly one saved Note On; all times and expression samples
  remained identical.
- Public-API recording, loop playback, live plus clip output allocation, session reopening,
  clip pad launch, feedback, launch quantization, duplication, one-step undo, retrospective
  capture, overdub and basic note-selection/edit commands. UI exposes the basic workflows.
- Firmware-backed Rust integration test records a three-note expressive phrase, reopens it,
  launches from a firmware user-mode contact and checks the actual green playing LED.
- Core builds without firmware/C++; native and UI build without Tracker. Local fmt/clippy,
  contract and firmware checks are runnable independently of GitHub CI.

## Still required before M3/v0.1 acceptance

The working vertical demonstration is not the complete release matrix. No v0.1 tag or
published release has been created. [CI run 38995410528](https://github.com/crestenstclair/crest-linnstrument/actions/runs/38995410528)
passed Rust, the Linux desktop build, the unmodified ARM baseline, and native firmware on
Linux/macOS, including UBSan. Edge calibration/touch guards and their fidelity limits are
documented in the firmware notes; physical equivalence is not implied by CI.

1. Broaden input/output overload recovery and input-profile selection in the host. Explicit
   destination MPE, ordinary-polyphonic and monophonic profiles now report capabilities,
   validate required polyphony/bend range, and persist in the host document. Ordinary output
   preserves key pressure and explicitly rejects or discards unrepresentable pitch/timbre;
   overlapping identical pitches are rejected. Monophonic last-note priority restores held
   notes with their latest expression. Profiles cannot change during transport/recording.
   Disconnect after output rejection releases actual allocated voices. Ordinary polyphonic
   input, key/channel expression, sustain and incoming RPN bend/zone negotiation are tested;
   the host input profile remains explicit lower MPE. Automatic device discovery is pending.
2. Complete control-panel/calibration, split-zone and endurance acceptance. Both sizes now
   pass concurrent X/Y/Z, capacity/reuse, foot-switch sustain, persisted octave and erased
   flash boot. Physical sensor behavior and USB delivery remain unqualified.
3. Repeatable automated desktop gesture/focus regression coverage. Native-clock linear
   attack/release shaping now feeds the sensor path for mouse, keyboard and scripts, with
   bounded durations, safe cancellation and script-tail ownership. Desktop keyboard attack
   and release were exercised; a 10,771-step pressure run replayed exactly. Configurable
   geometry-validated keyboard contacts/pressure keys now persist;
   physical key-code ownership survives modifier changes and suppresses repeat attacks.
   Stable contact/clip buttons preserve selection and focus during snapshot updates. Richer per-note inspector plots remain optional.
   Actual mouse/keyboard input, independent latched contacts, gesture export and exact replay
   were exercised manually in the Tauri app. Native-clock scripted multi-contact playback and
   cancellation are also implemented, with public scenario validation and firmware tests. A 1,267-step manual run replayed exactly. The final desktop build also ran its scripted
   phrase, reopened the included session, and launched/stopped it from a mouse-operated
   firmware pad, with actual LEDs changing from blue to green and back. Worker
   failure/recovery, live-plus-clip channel separation and reset preserving clips are tested.
4. Large-session/overdub stress, broader saturation tests and release packaging/runtime on
   Linux/Steam Deck. MIDI input overflow, failed session replacement and version rejection
   have focused tests. Capture remains bounded diagnostic history.
5. Keep repository CI passing and deliberately publish the accepted version with example sessions
   and compatibility documentation. A firmware-recorded example session is included. Current workflow creates development artifacts only.

OS MIDI transport is implemented as an optional midir backend. Physical devices and the
virtual firmware use the same adapter. On macOS, tests passed an actual CoreMIDI virtual
port round trip and firmware sensor → OS MIDI → standalone host, with LED feedback returning
through the firmware parser. The CLI supplies native scheduling, explicit routing, removal
checks and shutdown cleanup. Linux compilation is in CI; Linux runtime and physical device
qualification have not been claimed. The core still builds without MIDI, C++, or Tracker.

M0 contracts/firmware baseline and the principal M1 feasibility path are implemented.
M2 and M3 have useful working behavior but remain open against their complete gates.
The native-port feasibility uncertainty is substantially reduced; this does not qualify
physical hardware, every upstream feature, or the M3 release.

## Later ownership

Initial M4/M5 slices are now implemented: diatonic chord pads, extensions/suspensions,
parallel-scale borrowing, inversion/spread/register, bounded voice leading, bass routing,
all/top-voice expression, selected-chord strumplate, seeded Euclidean accompaniment,
independent pitch cycles, bounded humanization, live/selected harmony following and freeze.
Firmware tests on 128/200 record three expressive gestures as nine voices, reopen and launch
with actual firmware LED feedback. Public-contract tests cover atomic fan-out, held-note
safety, seeds, lane independence, freeze/undo and schema-1 migration. Desktop UI recording
saved nine generated voices with source provenance. Desktop generation, clip launch, degree
selection, freeze and save were exercised; the saved frozen pattern contained the expected
D-minor pitches and retained its original seed. Full M4/M5 acceptance remains open:
live strum/arpeggio articulation, custom chord capture, chord-clip leaders, conditions,
parameter locks, momentary transforms and scene morphing remain future work.

M6 integration must start
against the accepted M3 release in Crest Tracker's repository. Inspecting/changing its
engine and production integration was deliberately not done early. The confirmed native
backend is Rust, so a direct dependency is available; no speculative FFI wrapper was added.
M7 Linux/Deck audio, suspend/reconnect, controller workflow and physical comparison remain
separate qualification work. Optional reference sound is not connected; mandatory proof
uses captured musical/MIDI events.
