#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Hardware-dependent suites must be launched via their validating test scripts.
test_profile=${NAGAMETV_TEST_PROFILE:-dev}
case "$test_profile" in
    dev|release) ;;
    *) echo "NAGAMETV_TEST_PROFILE must be dev or release" >&2; exit 2 ;;
esac
cargo run --manifest-path rust/Cargo.toml --locked --profile "$test_profile" --features native_tests \
    -- --native-tests "$@"
