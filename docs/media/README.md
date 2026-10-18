# Manual video assets

These are the source assets for `../manual.html`. The HTML embeds all video, image, caption and session data; offline readers need only the HTML file.

- Software: Crest Performance v0.4.0, source `202bc47c39a70458563616f9683299b62cc0c4ed`.
- Firmware: `54bb1c9d355b1075b474ade0a0246b80ff7f3524`.
- Capture date: October 17, 2026, Pacific time.
- Capture environment: macOS, isolated documentation app bundle using the released application code and frontend, native firmware worker and MIDI/event capture sink.
- Inputs: real UI mouse and keyboard actions, plus the shipped **Play scripted phrase** control where identified in captions. Inputs traverse the application's normal contact/firmware path.
- Sound: none. No audio stream or reference instrument was added.
- Format: H.264 MP4, 1280 × 960, yuv420p, 20 fps output, fast-start metadata. Source capture rate varies; frames are repeated to fill the output timeline.
- Editing: application-window captures assembled in their recorded order. Pauses between steps removed; final frames held where needed to read captions. Titles and action captions added outside the original app image. No generated pad animations or substituted firmware LED output.
- Scope: end-user demonstrations, not performance measurements or replacements for the release acceptance suite.

Each MP4 has a JPEG poster and a WebVTT caption file. Captions are also printed in the video image, so they remain available when a local browser cannot load a separate caption track. The HTML includes timestamped text transcripts.

`demos.json` records durations, checksums and captions. `session-from-video-11.json` is the host session saved and reopened in Video 11. Its seven populated slots contain the strummed chord, expressive recording, retrospective capture, frozen bass, seeded melody, one-note parameter-lock clip and duplicate. Reopening leaves transport stopped. It does not load an audio instrument.

Several exercises reuse clips prepared in earlier videos. The initial state and selected slot are identified in each video's first steps. Follow the written chapter for a fresh-session procedure.

## Development surface redesign

`surface-play`, `surface-chords`, `surface-strumplate` and `surface-clips` are four additional
captioned videos of the instrument-first development UI. They are not v0.4.0 release footage.
They use actual application-window captures after real mouse and keyboard actions.
Intermediate keyboard movements are retained as discrete steps; frames are held for
caption reading. These are step demonstrations, not real-time recordings. No audio,
synthetic pad animation or replacement LED frame was added.

`surface-demos.json` records captions, durations and checksums. `surface-*.png` are
additional screenshots of the development application. `surface-reference.json` contains
read-only manual pad descriptions and LED colors captured through the native worker.
Regenerate the source snapshots with `cargo run -p linnstrument-sim --example surface-reference`
after building the worker, then run `node scripts/prepare-manual-reference.mjs`
and `python3 scripts/build-manual.py`. The reference uses default C-major harmony and the bundled
showcase session; its Clips frame has slot 1 playing. Selecting or inspecting a pad in
the manual never sends MIDI. Actual performance input remains in the desktop application.

The current `surface-chords` and `surface-strumplate` videos were recaptured after the
compact Crest Flower control-group adaptation. They show the actual User Firmware Mode surface, one degree/octave
bank, adjacent quality/extension and voicing rows, and UI root selection. The original
11 released-workflow videos retain their v0.4.0 layouts and labels. Custom chord crossings
retrigger; the stock Play-mode slide behavior is unchanged.

Edit `docs/manual-source.html` and run `python3 scripts/build-manual.py` to rebuild
the standalone HTML with embedded assets. The `manual-reference-*` files retain exact
images and text previously embedded in the released-workflow reference. Regenerating
the HTML does not record new UI footage.
