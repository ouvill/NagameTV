#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Separate processes preserve the missing-catalog test's fresh translator state.
bash scripts/run-native-tests.sh localization
bash scripts/run-native-tests.sh missing-catalog
