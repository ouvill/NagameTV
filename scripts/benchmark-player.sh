#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: $0 {mpv|gstreamer} [duration-seconds]" >&2
  exit 2
fi

backend="$1"
duration="${2:-600}"
stream_url="${MIRAKURUN_STREAM_URL:-http://192.168.3.3:40772/api/services/3203246080/stream}"
started_at="$(date +%Y%m%d-%H%M%S)"
result_dir="benchmark/${backend}-${started_at}"
mkdir -p "$result_dir"

case "$backend" in
  mpv)
    command=(mpv --no-config --force-window=yes "$stream_url")
    ;;
  gstreamer)
    command=(gst-launch-1.0 -e playbin3 uri="$stream_url")
    ;;
  *)
    echo "unknown backend: $backend" >&2
    exit 2
    ;;
esac

printf '%q ' "${command[@]}" > "$result_dir/command.txt"
printf '\n' >> "$result_dir/command.txt"
"${command[@]}" > "$result_dir/player.log" 2>&1 &
player_pid=$!

cleanup() {
  if kill -0 "$player_pid" 2>/dev/null; then
    kill -INT "$player_pid" 2>/dev/null || true
    wait "$player_pid" 2>/dev/null || true
  fi
}
trap cleanup EXIT INT TERM

sleep 5
if ! kill -0 "$player_pid" 2>/dev/null; then
  echo "$backend exited during startup; see $result_dir/player.log" >&2
  exit 1
fi

set +e
timeout "$duration" scripts/monitor-memory.sh \
  "$player_pid" 10 "$result_dir/memory.csv"
monitor_status=$?
set -e
if [[ $monitor_status -ne 0 && $monitor_status -ne 124 ]]; then
  exit "$monitor_status"
fi

echo "wrote $result_dir"
