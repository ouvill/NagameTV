#!/usr/bin/env bash
# Linux: diagnose the real desktop's Wayland input method without changing its
# settings. Run manually on the affected desktop, never as an automated GUI test.
set -euo pipefail
cd "$(dirname "$0")/.."

ime_backend=${1:-wayland}
case "$ime_backend" in
  wayland|ibus) ;;
  *) echo 'Usage: bash scripts/diagnose-ime.sh [wayland|ibus]' >&2; exit 2 ;;
esac

if [[ ! -x build/nagametv ]]; then
  echo 'Missing build/nagametv; run cmake --build build first.' >&2
  exit 1
fi
ime_display=${WAYLAND_DISPLAY:-wayland-0}
if [[ "$ime_display" != /* ]]; then
  if [[ -z ${XDG_RUNTIME_DIR:-} ]]; then
    echo 'Missing XDG_RUNTIME_DIR; run this on the affected Wayland desktop.' >&2
    exit 1
  fi
  ime_display="${XDG_RUNTIME_DIR}/${ime_display}"
fi
if [[ ! -S "$ime_display" ]]; then
  echo "Missing Wayland socket: ${ime_display}; a Wayland desktop is required." >&2
  exit 1
fi
# Keep repeated runs and concurrent diagnostic sessions separate.
ime_log=$(mktemp "build/ime-${ime_backend}-XXXXXX.log")
echo "Wayland IME diagnostic: ${ime_backend}; log: ${ime_log}"
echo 'Reproduce shortcuts, then close the app. Debug logs can contain typed text.'
env QT_QPA_PLATFORM=wayland QT_IM_MODULE="$ime_backend" QT_IM_MODULES="$ime_backend" \
    RUST_LOG=info,qt=debug \
    QT_LOGGING_RULES='qt.qpa.wayland.debug=true;qt.qpa.wayland.textinput.debug=true;qt.qpa.input.methods.debug=true;qt.quick.focus.debug=true' \
    ./build/nagametv 2>&1 | tee "$ime_log"
