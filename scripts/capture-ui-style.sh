#!/usr/bin/env bash
# Generate images for human review. This is not a visual regression test.
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/gui-test-session.sh
mkdir -p build/ui-review
QT_QPA_PLATFORM=xcb QSG_RHI_BACKEND=opengl \
    bash scripts/run-native-tests.sh channel-views -input tests/ui-review "$@"
