#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# This public entry point creates and validates its own real-GPU GUI session.
source scripts/gui-test-session.sh
mkdir -p build/ui-review
QT_QPA_PLATFORM=xcb QSG_RHI_BACKEND=opengl \
    bash scripts/run-native-tests.sh channel-views -input tests/ui-gallery "$@"
