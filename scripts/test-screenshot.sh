#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -z ${DISPLAY:-} ]]; then
    echo "DISPLAY is missing; screenshot tests require an X11 display." >&2
    exit 1
fi
if [[ ! -e /dev/nvidiactl ]] && ! compgen -G '/dev/dri/renderD*' >/dev/null; then
    echo "No GPU render device is available; screenshot tests require GPU access." >&2
    exit 1
fi
xdpyinfo -display "$DISPLAY" >/dev/null
renderer_info=$(glxinfo -B)
printf '%s\n' "$renderer_info"
if [[ $renderer_info == *llvmpipe* || $renderer_info == *softpipe* || $renderer_info == *"Software Rasterizer"* ]]; then
    echo "Only a software OpenGL renderer is available; GPU validation failed." >&2
    exit 1
fi
screenshot_test_dir=$(mktemp -d)
trap 'rm -rf "$screenshot_test_dir"' EXIT
mkdir -p "$screenshot_test_dir/rust/qml" "$screenshot_test_dir/tests/screenshots"
cp rust/qml/ScreenshotCapture.qml "$screenshot_test_dir/rust/qml/"
cp tests/screenshots/tst_ScreenshotCapture.qml "$screenshot_test_dir/tests/screenshots/"
qt_bins=$(pkg-config --variable=bindir Qt6Core)
QT_QPA_PLATFORM=xcb QSG_RHI_BACKEND=opengl "$qt_bins/qmltestrunner" \
    -input "$screenshot_test_dir/tests/screenshots" "$@"
