# Standalone Linux package

The `candidate-artifacts` workflow builds a Linux x86_64 archive on Ubuntu 24.04.
Tagged standalone releases promote the exact tested archive; this does not qualify Steam Deck.
The archive contains
the standalone host, simulator, replay tool, firmware worker, both ideal flash fixtures,
example sessions, contact scripts, documentation and third-party license notices.
`build-info.json` identifies the repository revision, Rust version, lockfile hash and
firmware/patch/calibration provenance. `dependencies.json` lists resolved dependency
versions and declared licenses. `SHA256SUMS` covers the extracted files.

Download an archive from the matching GitHub release, or unpack the
`development-candidate-linux-x86_64` workflow artifact. Verify
the archive checksum, and extract the tarball. In the extracted directory:

```sh
sha256sum --quiet -c SHA256SUMS
./linnstrument-sim
```

The worker and flash files are found beside the simulator executable. No Rust/C++ tools,
source checkout or network connection are needed at runtime. The system must provide
GTK 3, WebKitGTK 4.1, ALSA, librsvg and their runtime dependencies. On Ubuntu 24.04:

```sh
sudo apt-get install libwebkit2gtk-4.1-0 libgtk-3-0t64 libasound2t64 librsvg2-2
```

This is a dynamically linked native archive. Compatibility with older glibc systems,
SteamOS packaging, audio, gamepad operation and suspend/resume is not implied. A display
is required for the simulator; the capture host remains headless:

```sh
./crest-performance-host inspect examples/expressive-phrase.json
./crest-performance-host inspect examples/performance-showcase.json
./crest-performance-host demo
LINNSTRUMENT_MODEL=128 ./linnstrument-sim
```

The package workflow extracts into a fresh temporary directory, verifies every checksum,
opens both example sessions with the CLI, hides the build-tree firmware assets, and runs
the same real keyboard/mouse desktop acceptance scenario against the packaged executable.
Screenshots and observations are retained separately as `package-acceptance-linux`.
No release or Git tag is created by this workflow. Publication attaches the tested archive
without repacking; its build-info retains the candidate assembly provenance.

To build a candidate locally, use a clean committed checkout, build the firmware worker
with `scripts/build-native.sh`, build both Rust applications with the release profile and
their `midi`/`desktop` features, then run `python3 scripts/package-candidate.py`.
