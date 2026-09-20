#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# QCoreApplication, synthetic metadata and a private session bus; no hardware.
dbus-run-session -- bash scripts/run-native-tests.sh desktop-media
