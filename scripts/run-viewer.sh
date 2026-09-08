#!/usr/bin/env bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/log-options.sh"
parse_log_options run "$@"
if [[ "$print_log_settings" == 1 ]]; then show_log_settings; exit 0; fi
if [[ "$MIRAKURUN_HEAPTRACK" == 1 ]]; then
  exec bash "$(dirname "${BASH_SOURCE[0]}")/profile-memory.sh" "${viewer_args[@]}"
fi
export QT_QPA_PLATFORM=xcb
exec ./build/mirakurun-viewer "${viewer_args[@]}"
