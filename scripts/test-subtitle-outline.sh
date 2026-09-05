#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
test_dir=$(mktemp -d)
${CXX:-c++} -std=c++17 -fPIC -Irust/src tests/subtitle_outline.cpp \
    $(pkg-config --cflags --libs Qt6Core Qt6Gui Qt6Svg) -o "$test_dir/subtitle-outline-test"
"$test_dir/subtitle-outline-test"
