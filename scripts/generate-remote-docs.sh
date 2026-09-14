#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Tested tool versions: buf 1.73.0 and protoc-gen-doc 1.5.1.
for tool in buf protoc-gen-doc python3; do
    if ! command -v "$tool" >/dev/null; then
        echo "API documentation generation requires $tool on PATH; see docs/remote-control.md." >&2
        exit 1
    fi
done
buf format --diff --exit-code
buf lint
buf generate
# Normalize whitespace-only lines emitted by protoc-gen-doc for clean, repeatable diffs.
python3 - <<'PY'
from pathlib import Path

path = Path("docs/api/remote-control.md")
lines = (line if line.strip() else "" for line in path.read_text().splitlines())
path.write_text("\n".join(lines).rstrip("\n") + "\n")
PY
