#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
test_dir=$(mktemp -d)
trap 'rm -rf "$test_dir"' EXIT
# Requires a working display; use the same Qt platform as the viewer.
${CXX:-c++} -std=c++17 -fPIC -Irust/src tests/pointer_activity.cpp \
    $(pkg-config --cflags --libs Qt6Core Qt6Gui Qt6Quick Qt6Qml Qt6Test) \
    -o "$test_dir/pointer-activity-test"
"$test_dir/pointer-activity-test"
