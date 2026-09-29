#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/test-localization-catalog.py
# Separate processes preserve the missing-catalog test's fresh translator state.
bash scripts/run-native-tests.sh localization
bash scripts/run-native-tests.sh missing-catalog
