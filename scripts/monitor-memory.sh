#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 PID [interval-seconds]" >&2
  exit 2
fi

viewer_pid="$1"
interval="${2:-10}"
started_at="$(date +%Y%m%d-%H%M%S)"
output="${3:-memory-${viewer_pid}-${started_at}.csv}"

if [[ ! -r "/proc/${viewer_pid}/smaps_rollup" ]]; then
  echo "PID ${viewer_pid} is not readable" >&2
  exit 1
fi

echo "timestamp,rss_kib,pss_kib,threads,fds" > "$output"
while kill -0 "$viewer_pid" 2>/dev/null; do
  rss=$(awk '/^Rss:/ {print $2}' "/proc/${viewer_pid}/smaps_rollup")
  pss=$(awk '/^Pss:/ {print $2}' "/proc/${viewer_pid}/smaps_rollup")
  threads=$(awk '/^Threads:/ {print $2}' "/proc/${viewer_pid}/status")
  fds=$(find "/proc/${viewer_pid}/fd" -mindepth 1 -maxdepth 1 2>/dev/null | wc -l)
  printf '%s,%s,%s,%s,%s\n' "$(date --iso-8601=seconds)" "$rss" "$pss" "$threads" "$fds" >> "$output"
  sleep "$interval"
done

echo "wrote ${output}"
