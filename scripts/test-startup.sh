#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
startup_suite=${1:-startup}
case "$startup_suite" in
    startup|recording-pid-change|timeshift|screenshot-playback|video-processing)
        if (( $# > 1 )); then
            echo "This suite does not accept a fixture path" >&2; exit 2
        fi ;;
    recording-audit|recording-probe)
        if (( $# != 2 )) || [[ ! -f $2 ]]; then
            echo "Expected recording-audit or recording-probe followed by a TS path" >&2; exit 2
        fi ;;
    *) echo "Expected startup, timeshift, screenshot-playback, video-processing, recording-pid-change, recording-audit or recording-probe" >&2; exit 2 ;;
esac
startup_deinterlace=${NAGAMETV_DEINTERLACE:-yadif}
startup_default_qpa=xcb
if [[ ${startup_deinterlace,,} == va ]]; then
    startup_default_qpa=wayland
fi
startup_qpa=${NAGAMETV_TEST_QPA:-$startup_default_qpa}
case "$startup_qpa" in
    xcb|wayland|auto) ;;
    *) echo "NAGAMETV_TEST_QPA must be xcb, wayland or auto" >&2; exit 2 ;;
esac
source scripts/gui-test-session.sh
if [[ $startup_qpa == wayland || $startup_qpa == auto ]]; then
    if [[ -z ${WAYLAND_DISPLAY:-} || ! -S ${XDG_RUNTIME_DIR:-/nonexistent}/${WAYLAND_DISPLAY} ]]; then
        echo "Wayland tests require the private Wayland display." >&2
        exit 1
    fi
fi
# auto exercises Qt's selection with both private display endpoints available.
# Never inherit the host's QPA settings or change the isolated display routes.
startup_platform_env=(QT_QPA_PLATFORM="$startup_qpa")
if [[ $startup_qpa == auto ]]; then
    startup_platform_env=(-u QT_QPA_PLATFORM)
fi
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
    echo "No PulseAudio endpoint is available; startup tests require the validated virtual output." >&2
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
    echo "No PulseAudio sinks are available; startup tests require the validated virtual output." >&2
    exit 1
fi
startup_test_dir=$(mktemp -d)
trap 'rm -rf "$startup_test_dir"' EXIT
env -u NAGAMETV_SERVER -u NAGAMETV_SERVICE_ID -u NAGAMETV_AUTOPLAY \
    -u NAGAMETV_REMOTE_ENABLED -u NAGAMETV_REMOTE_ADDR -u NAGAMETV_REMOTE_PORT \
    "${startup_platform_env[@]}" \
    XDG_CONFIG_HOME="$startup_test_dir/config" XDG_CACHE_HOME="$startup_test_dir/cache" XDG_STATE_HOME="$startup_test_dir/state" \
    NAGAMETV_DIAGNOSTICS=0 NAGAMETV_AUDIO_SINK=pulsesink \
    QSG_RHI_BACKEND=opengl \
    bash scripts/run-native-tests.sh "$startup_suite" "${@:2}"
