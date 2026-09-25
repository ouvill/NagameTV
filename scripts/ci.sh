#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Linux CI in packaging/appimage/Dockerfile's ci stage. These suites use only
# CPU, QCoreApplication, in-memory images and local network/filesystem fixtures.
# GUI suites must use the separately validated real-GPU test environment.
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$PWD/build/ci-native/cargo}
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}

# The private D-Bus tests require an NSS entry even when Docker accepts a numeric
# --user. See docs/ci-release.md for the read-only passwd/group mounts.
getent passwd "$(id -u)" >/dev/null || {
  echo "CI: current UID has no passwd entry; see docs/ci-release.md Docker command." >&2
  exit 1
}

exec python3 scripts/test.py cpu "$@"
