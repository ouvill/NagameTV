#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Hardware-dependent suites must be launched via their validating test scripts.
exec python3 scripts/run-test-binary.py -- --native-tests "$@"
