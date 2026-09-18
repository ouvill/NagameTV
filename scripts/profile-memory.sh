#!/usr/bin/env bash
# Start profiling from process creation; retain the matching executable and logs.
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/log-options.sh"
parse_log_options profile "$@"
if [[ "$print_log_settings" == 1 ]]; then show_log_settings; exit 0; fi
set -- "${viewer_args[@]}"
if [[ "$NAGAMETV_HEAPTRACK" == 0 ]]; then
  export QT_QPA_PLATFORM=xcb
  exec ./build/nagametv "$@"
fi
for tool in heaptrack xdpyinfo glxinfo pactl timeout; do
  command -v "$tool" >/dev/null || { echo "Required command missing: $tool" >&2; exit 1; }
done
[[ -n "${DISPLAY:-}" ]] || { echo 'DISPLAY is missing; X11 display is required for playback.' >&2; exit 1; }
# Validate the real display, GPU and audio server. Do not substitute virtual devices.
timeout 10 xdpyinfo >/dev/null
if ! compgen -G '/dev/dri/renderD*' >/dev/null && [[ ! -e /dev/nvidia0 ]]; then
  echo 'GPU device is missing; OpenGL playback requires GPU access.' >&2
  exit 1
fi
graphics=$(timeout 15 glxinfo -B)
if [[ "$graphics" != *'direct rendering: Yes'* ]] || [[ "$graphics" =~ llvmpipe|softpipe|Software\ Rasterizer ]]; then
  echo 'Hardware OpenGL validation failed.' >&2
  exit 1
fi
timeout 10 pactl info >/dev/null
[[ -x build/nagametv ]] || { echo 'Run workshop run dev build first.' >&2; exit 1; }
mkdir -p benchmark/memory
capture_dir=$(mktemp -d "$PWD/benchmark/memory/session-$(date +%Y%m%d-%H%M%S)-XXXXXX")
cp build/nagametv "$capture_dir/nagametv"
printf '%s\n' "$graphics" > "$capture_dir/graphics.txt"
heaptrack --version > "$capture_dir/heaptrack-version.txt"
ldd "$capture_dir/nagametv" > "$capture_dir/libraries.txt"
git rev-parse HEAD > "$capture_dir/revision.txt"
git diff --binary HEAD > "$capture_dir/working-tree.patch"
printf '%s\0' "$@" > "$capture_dir/arguments.nul"
show_log_settings > "$capture_dir/log-settings.txt"
# Separate state logs keep automatic cleanup from removing evidence of this run.
export XDG_STATE_HOME="$capture_dir/state"
export QT_QPA_PLATFORM=xcb
echo "Memory capture: $capture_dir" >&2
echo 'Close the application normally to finish the heaptrack recording.' >&2
exec heaptrack --record-only -o "$capture_dir/heaptrack" "$capture_dir/nagametv" "$@"
