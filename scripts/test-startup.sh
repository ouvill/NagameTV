#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# The production Main.qml creates the real video item and audio output.
if [[ -z ${DISPLAY:-} ]]; then
    echo "DISPLAY is missing; startup tests require an X11 display." >&2
    exit 1
fi
if [[ ! -e /dev/nvidiactl ]] && ! compgen -G '/dev/dri/renderD*' >/dev/null; then
    echo "No GPU render device is available; startup tests require GPU access." >&2
    exit 1
fi
if [[ -z ${PULSE_SERVER:-} && ! -S ${XDG_RUNTIME_DIR:-/nonexistent}/pulse/native ]]; then
    echo "No PulseAudio endpoint is available; startup tests require real audio output." >&2
    exit 1
fi
xdpyinfo -display "$DISPLAY" >/dev/null
renderer_info=$(glxinfo -B)
printf '%s\n' "$renderer_info"
if [[ $renderer_info == *llvmpipe* || $renderer_info == *softpipe* || $renderer_info == *"Software Rasterizer"* ]]; then
    echo "Only a software OpenGL renderer is available; GPU validation failed." >&2
    exit 1
fi
pactl info >/dev/null
audio_sinks=$(pactl list short sinks)
if [[ -z $audio_sinks ]]; then
    echo "No PulseAudio sinks are available; startup tests require real audio output." >&2
    exit 1
fi
startup_test_dir=$(mktemp -d)
trap 'rm -rf "$startup_test_dir"' EXIT
env -u MIRAKURUN_SERVER -u MIRAKURUN_SERVICE_ID -u MIRAKURUN_AUTOPLAY \
    XDG_CONFIG_HOME="$startup_test_dir/config" XDG_STATE_HOME="$startup_test_dir/state" \
    MIRAKURUN_DIAGNOSTICS=0 MIRAKURUN_AUDIO_SINK=pulsesink \
    QT_QPA_PLATFORM=xcb QSG_RHI_BACKEND=opengl \
    bash scripts/run-native-tests.sh startup
