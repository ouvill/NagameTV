#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

# Hardware-free: use the same Qt and generated CXX-Qt metadata as the build.
# cargo metadata resolves relative CARGO_TARGET_DIR and Cargo configuration too.
qml_target_dir=$(cargo metadata --manifest-path rust/Cargo.toml --locked --no-deps --format-version 1 \
    | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])')
qml_import_dir="$qml_target_dir/cxxqt/qml_modules"
if [[ ! -f "$qml_import_dir/MinimalViewer/qmldir" ]]; then
    echo "QML type information missing: $qml_import_dir/MinimalViewer/qmldir" >&2
    echo "Build the application with the same CARGO_TARGET_DIR before running this check." >&2
    exit 1
fi
qml_qmake=${QMAKE:-qmake6}
qml_host_bins=$("$qml_qmake" -query QT_HOST_BINS)
qml_files=(rust/qml/*.qml)
echo "Checking all ${#qml_files[@]} production QML files (including evaluation components)."
# Tooling-only GStreamer metadata is never placed on the application's import path.
# Ignore local lint settings so warnings cannot be silently downgraded in CI.
"$qml_host_bins/qmllint" --ignore-settings --max-warnings 0 \
    -I "$qml_import_dir" -I tools/qmltypes "${qml_files[@]}"
