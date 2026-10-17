#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/generate-sketch.py
"${CXX:-c++}" -std=c++17 -O2 -fwrapv -fshort-enums -funsigned-char \
  -I firmware/native/include -I target/firmware-native/generated \
  firmware/native/worker.cpp firmware/native/peripherals.cpp \
  -o target/firmware-native/linnstrument-worker
for model in 128 200; do
  target/firmware-native/linnstrument-worker "$model" "target/firmware-native/ideal-$model.bin" --make-fixture
done
python3 scripts/firmware-manifest.py
