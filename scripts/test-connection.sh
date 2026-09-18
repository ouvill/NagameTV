#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# QCoreApplication and local HTTP fixtures; no display, GPU or audio is used.
connection_test_dir=$(mktemp -d)
trap 'rm -rf "$connection_test_dir"' EXIT
env -u NAGAMETV_SERVER -u NAGAMETV_SERVICE_ID -u NAGAMETV_AUTOPLAY \
    -u NAGAMETV_REMOTE_ENABLED -u NAGAMETV_REMOTE_ADDR -u NAGAMETV_REMOTE_PORT \
    XDG_CONFIG_HOME="$connection_test_dir/config" \
    XDG_STATE_HOME="$connection_test_dir/state" NAGAMETV_DIAGNOSTICS=0 \
    bash scripts/run-native-tests.sh connection
