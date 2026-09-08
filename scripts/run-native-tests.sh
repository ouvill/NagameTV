#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Hardware-dependent suites must be launched via their validating test scripts.
cargo run --manifest-path rust/Cargo.toml --locked --features native_tests \
    -- --native-tests "$@"
