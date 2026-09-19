#!/usr/bin/env bash
# Linux: capture only through the project's real-GPU private display/audio mode.
set -euo pipefail
cd "$(dirname "$0")/.."
for command in import xdotool; do
    if ! command -v "$command" >/dev/null; then
        echo "Missing publicity dependency: $command; see docs/publicity.md" >&2
        exit 1
    fi
done
if [[ ! -x build/nagametv || ! -s build/publicity/demo.ts ]]; then
    echo 'Build nagametv and run python3 scripts/publicity/prepare.py first.' >&2
    exit 1
fi
source scripts/gui-test-session.sh
python3 scripts/publicity/capture.py
