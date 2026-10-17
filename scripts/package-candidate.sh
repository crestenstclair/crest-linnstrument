#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p target/candidate/licenses
cp target/release/crest-performance-host target/release/linnstrument-sim target/release/linnstrument-replay target/firmware-native/linnstrument-worker target/candidate/
cp target/firmware-native/ideal-*.bin target/firmware-native/ideal-*.json target/candidate/
cp LICENSE README.md target/candidate/
cp third_party/linnstrument-firmware/LICENSE.txt target/candidate/licenses/linnstrument-firmware.txt
cp 'third_party/linnstrument-firmware/libraries/DueFlashStorage/licence (MIT).txt' target/candidate/licenses/DueFlashStorage.txt
cp -R docs examples target/candidate/
