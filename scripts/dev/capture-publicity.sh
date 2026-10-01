#!/usr/bin/env bash
# Linux: capture only through the project's real-GPU private display/audio mode.
set -euo pipefail
cd "$(dirname "$0")/../.."
for command in import xdotool; do
    if ! command -v "$command" >/dev/null; then
        echo "Missing publicity dependency: $command; see docs/publicity.md" >&2
        exit 1
    fi
done
if [[ ! -x build/nagametv || ! -s build/publicity/demo.ts ]]; then
    echo 'Build nagametv and run python3 -m scripts.dev.publicity.prepare first.' >&2
    exit 1
fi
exec python3 -m scripts.testing.gui_session -- python3 -m scripts.dev.publicity.capture
