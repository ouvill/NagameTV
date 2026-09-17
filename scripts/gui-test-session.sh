#!/usr/bin/env bash
# Sourced by GUI suites after changing to the repository root. The launcher
# alone publishes this context after GPU and virtual audio validation.
if [[ -z ${MIRAKURUN_GUI_SESSION_DIR:-} ]]; then
    exec python3 scripts/run-gui-tests.py -- bash "scripts/${0##*/}" "$@"
fi

# Catch stale sessions and changed connection variables before opening a window.
# The suites still perform their own display/GPU checks inside this session.
if [[ ! -f $MIRAKURUN_GUI_SESSION_DIR/display ]] ||
   [[ ${XDG_RUNTIME_DIR:-} != "$MIRAKURUN_GUI_SESSION_DIR" ]] ||
   [[ ${DISPLAY:-} != "$(cat "$MIRAKURUN_GUI_SESSION_DIR/display")" ]] ||
   [[ ${WAYLAND_DISPLAY:-} != litv-wayland ]] ||
   [[ ${PULSE_SERVER:-} != "unix:$MIRAKURUN_GUI_SESSION_DIR/pulse.sock" ]] ||
   [[ ${DBUS_SESSION_BUS_ADDRESS:-} != "unix:path=$MIRAKURUN_GUI_SESSION_DIR/bus" ]]; then
    echo "Invalid or closed isolated GUI session; rerun the public test script." >&2
    exit 1
fi
