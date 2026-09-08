#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# QCoreApplication with in-memory raster geometry: no display/GPU/audio.
exec bash scripts/run-native-tests.sh subtitle-outline "$@"
