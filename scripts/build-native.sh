#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
compiler_flags=(-std=c++17 -O2 -fwrapv -fshort-enums -funsigned-char)
if [[ $# == 1 && "$1" == --sanitize ]]; then
  compiler_flags+=(-fsanitize=undefined -fno-sanitize-recover=all -g -fno-omit-frame-pointer)
elif [[ $# != 0 ]]; then
  echo 'usage: scripts/build-native.sh [--sanitize]' >&2
  exit 2
fi
python3 scripts/generate-sketch.py
"${CXX:-c++}" "${compiler_flags[@]}" \
  -I firmware/native/include -I target/firmware-native/generated \
  firmware/native/worker.cpp firmware/native/peripherals.cpp \
  -o target/firmware-native/linnstrument-worker
for model in 128 200; do
  target/firmware-native/linnstrument-worker "$model" "target/firmware-native/ideal-$model.bin" --make-fixture
done
python3 scripts/firmware-manifest.py
