# Delivery status — October 17, 2026, Pacific

Stable version `0.2.0` carries API 9, session schema 8 and host document envelope 1.
Development version `0.3.0-dev.1` retains those versions; its additions are not yet tagged.
The first
standalone integration baseline, `v0.1.0`, remains API 2/session 2. Acceptance belongs to
the corresponding release tag and commit; an untagged checkout remains a candidate. There is no dependency on Crest Tracker and
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
open: parameter locks, momentary transforms and scene morphing remain future work.

M6 starts in Crest Tracker's own repository against the accepted M3 tag. Its native backend
is Rust, so a direct dependency is available; no speculative FFI wrapper was added. Tracker
owns its adapter, engine capability changes, production UI, project mapping and later Deck
package. M4/M5 can continue here while Tracker remains pinned to its accepted version.

Tracker's separate `feat/performance-v0.1` branch now consumes API-2 patch commit
`2e86f2af04c456ad1ac0d989c3ab5d2e3783d909`. Its retrospective capture integration at
`f56f6fe3e4f6820f2fd59f950201fbbb2c4cb4ea` passed [full CI](https://github.com/crestenstclair/crest-tracker/actions/runs/39014158393),
including native Sample monitoring/capture/reopen/replay and the macOS bundle. M6 remains
open: simultaneous performance clips, external performance routes and Linux MIDI/audio
clock correlation still need integration work. Deck and physical-device qualification
remain separate. This library has no dependency on that checkout or its application.

## Timed strum

Version 0.2.0 uses API 9/session 8; Tracker continues to consume its separately
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


## Learned chord bank

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


The learned-bank candidate passed the complete standalone CI run, including the Linux
webview scenario, both native firmware platforms and the ARM baseline. Native macOS
keyboard input also learned pitches 42/45/49, triggered the bank from one firmware pad,
recorded three notes with 21 expression events and nine source gestures, and reopened the
schema-4 session stopped with zero held voices. These checks precede the v0.2.0 tag.


## Repeating arpeggio

Ascending and descending held-chord arpeggios use the native musical clock, configurable
beat spacing and a bounded gate. Each repeat gets a fresh note identity and retains its
trigger's fixed voicing/expression policy. Bass stays sustained; strumplate stays manual.
Core acceptance covers equal-time release ordering, independent X/Y/Z across three
triggers, silent gaps, top-voice policy, stop/disconnect/mode cleanup, output retries,
full recording buffers and exact save/replay beside live MPE. API/session 5 and harmony
algorithm 4 explicitly version the new behavior; schemas 1–4 migrate without changing
rendered events. Tracker keeps its independently pinned API-2 dependency.

Both native firmware geometries run three actual expressive sensor contacts through the
stock MIDI path, render repeating library notes, verify independently calculated pitch,
attack and gate times, then save/reopen/replay exact events. The standalone host exposes
arpeggio direction, step spacing and gate; Linux desktop acceptance includes a held mouse
contact, expression and session reopening. These checks precede the v0.2.0 tag.

Native macOS desktop acceptance selected ascending arpeggio, latched a keyboard contact,
changed X/Y/Z, released, finished, saved and reopened. The saved take contains 266 attacks
and 266 releases, independently checked 60,000-tick spacing, gates no longer than 30,000
ticks, four sounding expression events and nine source gestures. Initial expression on
subsequent notes also preserves updates made during silence. Reopening leaves zero held
voices in ordinary Play mode. No physical hardware was used.

The new release-mode workload runs three arpeggio triggers alongside six looping notes,
with 30 ms steps, 20 ms gates and independent X/Y/Z at 100 Hz for three virtual minutes.
On this macOS/aarch64 machine, 17,900 measured batches had p50 583 ns, p95 958 ns,
p99 1,584 ns and maximum 42,583 ns; none reached 1 ms. All 15,120 attacks had matching
releases with no MPE channel collisions. Sensing, transport, files, UI and audio are
excluded; this does not qualify Steam Deck latency. CI publishes the same workload.


## Conditional playback

API/session 6 adds per-note every-Nth-loop, Fill and Not Fill rules. Stored raw notes and
expression remain unchanged. Attack decisions own complete note lifetimes; later Fill
changes cannot create orphan expression or lose releases. Rules survive timing edits,
track-remapped duplication, overdub, Undo and save/reopen. Schemas 1–5 migrate losslessly;
false older labels carrying conditions are rejected. Fill is momentary and reopens released.

Core tests cover repeated pitches, independent expression, capacity retries, queued starts,
relaunch counters, live notes, cleanup and malformed sessions. Both native firmware sizes
record the stock three-finger scenario, apply rules through the typed bridge, save/reopen,
launch through an actual sensor pad and check independently specified loop/pitch/timing
expectations. Decoded LEDs stay green during conditional silence and blue after pad stop.
The standalone note editor exposes the rules and a pointer/keyboard Fill hold with focus
recovery. This feature is included in v0.2.0; Tracker stays on API 2.


The conditional candidate passed full standalone CI [39015475727](https://github.com/crestenstclair/crest-linnstrument/actions/runs/39015475727),
including real Linux keyboard holds, silence before/after Fill, focus-loss recovery,
condition editing/Undo/save/reopen, both native firmware platforms and the ARM baseline.
Local macOS UI verification additionally edited fourth-loop/Fill rules and reopened the
schema-6 session. The standalone baseline tag remains unchanged.

## Humanized chords

API/session 7 adds seeded positive attack jitter and bounded velocity variation for held
chords. Each trigger chooses deterministic per-tone variation; voice ownership, top-voice
expression and source provenance survive reordered attacks and entirely delayed chords.
Early release cancels waiting tones. Bass and manual strumplate retain immediate attacks.
Harmony algorithm 5 and pattern algorithm 4 accept this provenance, while historical
algorithms retain their original constraints and rendered events. Schemas 1–6 migrate
losslessly; new settings hidden under old schema/algorithm labels reject explicitly.

Fixed independent vectors check exact delays and velocities. Core tests cover pending
expression, complete source gestures, recording reservations, early release, capacity,
cleanup, separate bass, top-voice policy and exact save/replay. Both native firmware sizes
record three sensor-generated triggers and independently check nine delayed attacks and
velocity changes, expression and exact persistence/replay. Desktop acceptance adds a held
mouse chord, all three expressive dimensions and session reopening. This feature is included in v0.2.0, independent of Tracker's API-2 integration.

The humanized release-mode workload ran three repeating chord triggers alongside six
looping notes, with X/Y/Z at 100 Hz for three virtual minutes on macOS/aarch64. The 17,900
measured batches had p50 583 ns, p95 1,000 ns, p99 1,708 ns and max 32,917 ns; none reached
1 ms. All 2,160 attacks had matching releases. The timed work excludes sensing, OS MIDI,
UI, files and audio; this is local processing evidence, not Deck latency qualification.

The humanized candidate passed [full standalone CI 39016098434](https://github.com/crestenstclair/crest-linnstrument/actions/runs/39016098434),
including a real Linux mouse contact, X/Y/Z, fixed seeded timing/velocity expectations,
session reopening, both native firmware platforms and the unchanged ARM baseline.
`examples/performance-showcase.json` adds a reproducible three-destination bank built
through public APIs from the existing firmware phrase: expressive humanized chords,
following Euclidean bass and Fill-gated accompaniment. The builder checks 108 balanced
attacks/releases, destination isolation, Fill silence and exact persistence. It remains
usable with only capture sinks and does not require a sound engine or Tracker checkout.


## Chord-clip harmony sources

API/session 8 adds a persisted choice of live/selected harmony or recorded chord-clip
intent. Followers resolve source gestures at each attack, independently of launch order
and delayed source audio. Queued/stopped sources use selected harmony; loop pickup gaps
retain the previous iteration's final chord. Held follower voices keep pitch and expression.
Generate and Freeze use the active target. Playback uses a control-thread-built binary
lookup index, rebuilt on commit, Undo and reopen. Schemas 1–7 migrate losslessly and reject
new clip-source state hidden under an earlier version.

Six focused core scenarios verify timing, expression ownership, launch order, queue/stop/
relaunch, simultaneous/unsorted provenance, Freeze, replacement/Undo, persistence and invalid
source rejection. Both firmware geometries record I/V from sensor contacts, save/reopen,
launch source and follower from firmware pads, compare independently specified follower
pitches across loops, then stop the source and verify D-minor fallback with independent
firmware LEDs. Desktop and package verification passed; the matching CI runs are linked below.


The API-2 overdub patch preserves the original raw stream, zero-duration notes and closing
expression, assigns fresh take identities and retains exact Undo. It passed independent
[baseline CI 39017257775](https://github.com/crestenstclair/crest-linnstrument/actions/runs/39017257775).
Tracker adopted this pin with seven adapter contracts and 261 application tests passing
locally. Development full [CI 39017632570](https://github.com/crestenstclair/crest-linnstrument/actions/runs/39017632570)
and extracted-package [CI 39017800242](https://github.com/crestenstclair/crest-linnstrument/actions/runs/39017800242)
passed the three-route showcase, firmware-pad launches, held Fill, independent stop and
firmware LED checks. These runs precede the API-8 harmony-source candidate.


The chord-source candidate passed [full CI 39018148143](https://github.com/crestenstclair/crest-linnstrument/actions/runs/39018148143)
and [extracted-package CI 39018207872](https://github.com/crestenstclair/crest-linnstrument/actions/runs/39018207872).
Real desktop actions select and reopen the source, launch three clips through firmware
pads, verify the leader overrides selected harmony, and switch sources during playback.

## Temporary clip mute

API 9 adds transient per-player mute; persistence remains schema 8. It releases only the
selected clip's voices and suppresses attacks while retaining loop phase, original data,
Undo and harmonic leadership. Unmute resumes future attacks; old notes never revive.
Queue-pressure rejection is atomic, and stop/mode/disconnect/relaunch clear mute.

Five core scenarios verify live/other-clip isolation, expression/release ownership,
queued launch phase, output-pressure retry, safety transitions, persistence and Undo.
Both firmware sizes also mute/unmute a chord leader: its independent follower continues
I/V, source MIDI goes silent, decoded LEDs turn cyan and return green, and future source
attacks resume. The keyboard-hold desktop path passed [CI 39018873909](https://github.com/crestenstclair/crest-linnstrument/actions/runs/39018873909).
Release-commit and package gates are rerun before publication.


After the mute lifecycle change, the existing three-minute release benchmarks still
balanced every note: 546 attacks/releases for six live plus six looping notes (p50 667 ns,
p95 959 ns, p99 1,334 ns, max 28,375 ns), and 2,160 for humanized chords plus looping notes
(p50 542 ns, p95 1,000 ns, p99 1,417 ns, max 15,833 ns). Neither macOS/aarch64 run had a
batch at or above 1 ms. These measure the existing expressive workload after the change,
not total latency, Deck performance or a dedicated mute-toggle workload.

## Controller harmony selection (development)

The standalone Clips layout now keeps launch/stop on the bottom row and assigns I–VII
to the first seven top-row pads. Selection uses existing harmony commands, takes over
from a recorded leader, and preserves held notes, clip phase, fixed pitches and saved
performances. Firmware LEDs show the effective degree in green and other degrees in blue.
Three protocol fixtures check ownership, persistence, unused/released pads and atomic
rejection. Both native firmware geometries exercise all seven degrees through actual
sensor input while source/follower clips run, with independent pitch and decoded LED
expectations. The desktop suite adds a real mouse selection and canvas-pixel check;
that updated desktop scenario still requires its CI result before qualification.
