#!/usr/bin/env bash
# Sourced by GUI suites after changing to the repository root. The launcher
# alone publishes this context after GPU and virtual audio validation.
if [[ -z ${NAGAMETV_GUI_SESSION_DIR:-} ]]; then
    exec python3 scripts/run-gui-tests.py -- bash "scripts/${0##*/}" "$@"
fi

# Catch stale sessions and changed connection variables before opening a window.
# The suites still perform their own display/GPU checks inside this session.
if [[ ! -f $NAGAMETV_GUI_SESSION_DIR/display ]] ||
   [[ ! -f $NAGAMETV_GUI_SESSION_DIR/pulse-server ]] ||
   [[ ! -f $NAGAMETV_GUI_SESSION_DIR/pulse-sink ]] ||
   [[ ${XDG_RUNTIME_DIR:-} != "$NAGAMETV_GUI_SESSION_DIR" ]] ||
   [[ ${DISPLAY:-} != "$(cat "$NAGAMETV_GUI_SESSION_DIR/display")" ]] ||
   [[ ${WAYLAND_DISPLAY:-} != nagametv-wayland ]] ||
   [[ ${PULSE_SERVER:-} != "$(cat "$NAGAMETV_GUI_SESSION_DIR/pulse-server")" ]] ||
   [[ ${PULSE_SINK:-} != "$(cat "$NAGAMETV_GUI_SESSION_DIR/pulse-sink")" ]] ||
   [[ ${PULSE_SOURCE:-} != "$PULSE_SINK.monitor" ]] ||
   [[ ${DBUS_SESSION_BUS_ADDRESS:-} != "unix:path=$NAGAMETV_GUI_SESSION_DIR/bus" ]]; then
    echo "Invalid or closed isolated GUI session; rerun the public test script." >&2
    exit 1
fi
