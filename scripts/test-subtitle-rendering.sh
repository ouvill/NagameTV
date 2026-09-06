#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# This suite uses a real X11 window and OpenGL. Do not substitute a renderer when
# the Workshop display or GPU is unavailable; report the missing resource instead.
if [[ -z ${DISPLAY:-} ]]; then
    echo "DISPLAY is missing; the subtitle rendering tests require an X11 display." >&2
    exit 1
fi
if [[ ! -e /dev/nvidiactl ]] && ! compgen -G '/dev/dri/renderD*' >/dev/null; then
    echo "No GPU render device is available; the subtitle rendering tests require GPU access." >&2
    exit 1
fi
xdpyinfo -display "$DISPLAY" >/dev/null
renderer_info=$(glxinfo -B)
printf '%s\n' "$renderer_info"
if [[ $renderer_info == *llvmpipe* || $renderer_info == *softpipe* || $renderer_info == *"Software Rasterizer"* ]]; then
    echo "Only a software OpenGL renderer is available; GPU validation failed." >&2
    exit 1
fi
test_dir=$(mktemp -d)
trap 'rm -rf "$test_dir"' EXIT
qt_libexec=$(pkg-config --variable=libexecdir Qt6Core)
"$qt_libexec/moc" tests/subtitle_rendering.cpp -o "$test_dir/subtitle_rendering.moc"
${CXX:-c++} -std=c++17 -fPIC -Irust/src -I"$test_dir" tests/subtitle_rendering.cpp \
    $(pkg-config --cflags --libs Qt6QuickTest Qt6Qml Qt6Gui) -o "$test_dir/subtitle-rendering-test"
QT_QPA_PLATFORM=xcb QSG_RHI_BACKEND=opengl "$test_dir/subtitle-rendering-test" -input tests/subtitles "$@"
