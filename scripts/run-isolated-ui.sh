#!/usr/bin/env bash
set -euo pipefail

display="${MIRAKURUN_TEST_DISPLAY:-:99}"
display_number="${display#:}"
display_number="${display_number%%.*}"
socket="/tmp/.X11-unix/X${display_number}"

if [[ -e "$socket" ]]; then
  echo "Isolated display $display is already in use: $socket" >&2
  exit 1
fi

Xvfb "$display" \
  -screen 0 "${MIRAKURUN_TEST_SCREEN:-1280x720x24}" \
  -nolisten tcp \
  -noreset &
xvfb_pid=$!
cleanup() {
  kill "$xvfb_pid" 2>/dev/null || true
  wait "$xvfb_pid" 2>/dev/null || true
}
trap cleanup EXIT INT TERM

for _ in {1..50}; do
  if DISPLAY="$display" xdpyinfo >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done
if ! DISPLAY="$display" xdpyinfo >/dev/null 2>&1; then
  echo "Xvfb did not become ready on $display" >&2
  exit 1
fi

renderer=$(DISPLAY="$display" LIBGL_ALWAYS_SOFTWARE=1 glxinfo -B 2>/dev/null \
  | sed -n 's/^OpenGL renderer string: //p')
if [[ -z "$renderer" ]]; then
  echo "Software OpenGL is unavailable on isolated display $display" >&2
  exit 1
fi
printf 'Isolated display: %s\nOpenGL renderer: %s\n' "$display" "$renderer"

env \
  DISPLAY="$display" \
  WAYLAND_DISPLAY= \
  QT_QPA_PLATFORM=xcb \
  QT_OPENGL=software \
  LIBGL_ALWAYS_SOFTWARE=1 \
  MIRAKURUN_AUDIO_SINK=fakesink \
  "${MIRAKURUN_VIEWER_BINARY:-./build/mirakurun-viewer}" "$@"
