#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."

# Linux CI in packaging/appimage/Dockerfile's ci stage. These suites use only
# CPU, QCoreApplication, in-memory images and local network/filesystem fixtures.
# GUI suites must use the separately validated real-GPU test environment.
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$PWD/build/ci-native/cargo}
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
echo "Cargo build jobs: $CARGO_BUILD_JOBS"

# The private D-Bus tests require an NSS entry even when Docker accepts a numeric
# --user. See docs/ci-release.md for the read-only passwd/group mounts.
getent passwd "$(id -u)" >/dev/null || {
  echo "CI: current UID has no passwd entry; see docs/ci-release.md Docker command." >&2
  exit 1
}

# The offline build-info fixture needs the same registry as the application,
# including on a runner without a restored download cache.
bash scripts/build/with-build-lock.sh cargo fetch --manifest-path rust/Cargo.toml --locked

# Match the distribution profile so dependencies are reused by packaging and
# the real EPGStation contract tests; local development defaults to dev.
exec python3 scripts/test.py cpu --profile release "$@"
