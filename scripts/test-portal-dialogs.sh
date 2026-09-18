#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
source scripts/gui-test-session.sh
# Linux only: exercise Qt's real portal plugin over a private D-Bus session.
if [[ -z ${DISPLAY:-} ]]; then
    echo "DISPLAY is missing; portal dialog tests require an X11 display." >&2
    exit 1
fi
if [[ ! -e /dev/nvidiactl ]] && ! compgen -G '/dev/dri/renderD*' >/dev/null; then
    echo "No GPU render device is available; portal dialog tests require GPU access." >&2
    exit 1
fi
xdpyinfo -display "$DISPLAY" >/dev/null
renderer_info=$(glxinfo -B)
if [[ $renderer_info == *llvmpipe* || $renderer_info == *softpipe* || $renderer_info == *"Software Rasterizer"* ]]; then
    echo "Only a software OpenGL renderer is available; GPU validation failed." >&2
    exit 1
fi
python3 -c 'import dbus; import gi'
portal_test_dir=$(mktemp -d)
trap 'rm -rf "$portal_test_dir"' EXIT
touch "$portal_test_dir/録画 #100%.ts"
mkdir "$portal_test_dir/キャプチャ #100%"
export VIEWER_PORTAL_TEST_DIR="$portal_test_dir"
export XDG_CONFIG_HOME="$portal_test_dir/config" XDG_DATA_HOME="$portal_test_dir/data"
export XDG_STATE_HOME="$portal_test_dir/state"
export QT_QPA_PLATFORM=xcb QSG_RHI_BACKEND=opengl
dbus-run-session -- bash -euo pipefail -c '
    python3 scripts/portal-test-service.py &
    service=$!
    trap '\''kill "$service"; wait "$service" || true'\'' EXIT
    for attempt in {1..100}; do
        [[ -e "$VIEWER_PORTAL_TEST_DIR/ready" ]] && break
        kill -0 "$service"
        sleep 0.05
    done
    test -e "$VIEWER_PORTAL_TEST_DIR/ready"
    bash scripts/run-native-tests.sh portal-dialogs
'
