#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/gui-test-session.sh
build_options=()
if [[ ${1:-} == --evaluation-legacy-comments && $# == 1 ]]; then
    build_options+=(--evaluation-legacy-comments)
elif [[ $# != 0 ]]; then
    echo "Usage: $0 [--evaluation-legacy-comments]" >&2
    exit 2
fi
if [[ -z ${DISPLAY:-} ]]; then
    echo "DISPLAY is missing; QML integration tests require an X11 display." >&2
    exit 1
fi
if [[ ! -e /dev/nvidiactl ]] && ! compgen -G '/dev/dri/renderD*' >/dev/null; then
    echo "No GPU render device is available; QML integration tests require GPU access." >&2
    exit 1
fi
xdpyinfo -display "$DISPLAY" >/dev/null
renderer_info=$(glxinfo -B)
if [[ $renderer_info == *llvmpipe* || $renderer_info == *softpipe* || $renderer_info == *"Software Rasterizer"* ]]; then
    echo "Only software OpenGL is available; hardware validation failed." >&2
    exit 1
fi
printf '%s\n' "$renderer_info"
QT_QPA_PLATFORM=xcb QSG_RHI_BACKEND=opengl \
    python3 scripts/run-test-binary.py "${build_options[@]}" -- --qml-tests
