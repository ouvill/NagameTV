#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Linux CI in packaging/appimage/Dockerfile's ci stage. These suites use only
# CPU, QCoreApplication, in-memory images and local network/filesystem fixtures.
# GUI suites must use the separately validated real-GPU test environment.
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$PWD/build/ci-native/cargo}
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-2}
export NAGAMETV_TEST_PROFILE=release

# The private D-Bus tests require an NSS entry even when Docker accepts a numeric
# --user. See docs/ci-release.md for the read-only passwd/group mounts.
getent passwd "$(id -u)" >/dev/null || {
  echo "CI: current UID has no passwd entry; see docs/ci-release.md Docker command." >&2
  exit 1
}

cargo fmt --manifest-path rust/Cargo.toml --check
python3 scripts/check-release-metadata.py
python3 scripts/flatpak-cargo-sources.py --check
for suite in scripts/test-*.py; do
  python3 "$suite"
done

python3 scripts/check-comment-sql.py

cargo test --manifest-path rust/Cargo.toml --release --locked
# Cargo does not run tests belonging to path dependencies of the application.
for crate in viewer-comments viewer-epg-events viewer-diagnostics viewer-remote tsreadex; do
  manifest="rust/crates/$crate/Cargo.toml"
  case "$crate" in
    viewer-comments|viewer-epg-events) features=(--features network) ;;
    viewer-diagnostics|viewer-remote|tsreadex) features=() ;;
  esac
  cargo test --manifest-path "$manifest" --release --locked "${features[@]}"
done

bash scripts/test-connection.sh
python3 scripts/check-cli.py "$CARGO_TARGET_DIR/release/nagametv"
bash scripts/test-desktop-media.sh
bash scripts/test-localization.sh
bash scripts/test-subtitle-outline.sh
