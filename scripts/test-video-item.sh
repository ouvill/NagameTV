#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if (( $# != 0 )); then
    echo "Usage: scripts/test-video-item.sh (runs all Rust video item checks)" >&2
    exit 2
fi
source scripts/gui-test-session.sh
# This suite loads the real Qt/GStreamer GUI plugin. Do not substitute a renderer when
# the Workshop display or GPU is unavailable; report the missing resource instead.
if [[ -z ${DISPLAY:-} ]]; then
    echo "DISPLAY is missing; the video item tests require an X11 display." >&2
    exit 1
fi
if [[ ! -e /dev/nvidiactl ]] && ! compgen -G '/dev/dri/renderD*' >/dev/null; then
    echo "No GPU render device is available; the video item tests require GPU access." >&2
    exit 1
fi
xdpyinfo -display "$DISPLAY" >/dev/null
renderer_info=$(glxinfo -B)
printf '%s\n' "$renderer_info"
if [[ $renderer_info == *llvmpipe* || $renderer_info == *softpipe* || $renderer_info == *"Software Rasterizer"* ]]; then
    echo "Only a software OpenGL renderer is available; GPU validation failed." >&2
    exit 1
fi
QT_QPA_PLATFORM=xcb QSG_RHI_BACKEND=opengl \
    cargo run --manifest-path rust/Cargo.toml --locked --features video_item_tests \
    -- --video-item-tests
