#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# QCoreApplication and local HTTP fixtures; no display, GPU or audio is used.
connection_test_dir=$(mktemp -d)
trap 'rm -rf "$connection_test_dir"' EXIT
env -u MIRAKURUN_SERVER -u MIRAKURUN_SERVICE_ID -u MIRAKURUN_AUTOPLAY \
    -u MIRAKURUN_REMOTE_ENABLED -u MIRAKURUN_REMOTE_ADDR -u MIRAKURUN_REMOTE_PORT \
    XDG_CONFIG_HOME="$connection_test_dir/config" \
    XDG_STATE_HOME="$connection_test_dir/state" MIRAKURUN_DIAGNOSTICS=0 \
    bash scripts/run-native-tests.sh connection
