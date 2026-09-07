#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# UI-only runs are explicitly selected, never a fallback after GPU validation fails.
test_mode=gpu
if [[ ${1:-} == --ui-only ]]; then
    test_mode=ui
    shift
fi
if [[ $test_mode == ui ]]; then
    echo "Subtitle UI-only test: software OpenGL is allowed; this is not GPU validation."
else
    echo "Subtitle GPU rendering test: a hardware renderer is required."
fi
if [[ -z ${DISPLAY:-} ]]; then
    echo "DISPLAY is missing; the subtitle rendering tests require an X11 display." >&2
    exit 1
fi
if [[ $test_mode == gpu && ! -e /dev/nvidiactl ]] && ! compgen -G '/dev/dri/renderD*' >/dev/null; then
    echo "No GPU render device is available; the subtitle rendering tests require GPU access." >&2
    exit 1
fi
xdpyinfo -display "$DISPLAY" >/dev/null
renderer_info=$(glxinfo -B)
printf '%s\n' "$renderer_info"
if [[ $test_mode == gpu ]] && [[ $renderer_info == *llvmpipe* || $renderer_info == *softpipe* || $renderer_info == *"Software Rasterizer"* ]]; then
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
