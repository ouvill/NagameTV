pragma ComponentBehavior: Bound
import QtQuick

// UI bindings only. Rust owns data validation, sorting, cursor, seek and pause.
QtObject {
    required property DanmakuOverlay overlay
    property real position: 0
    property bool playing: true
    function load(json: string): bool { return overlay.controller.load_timeline(json); }
    function seek(seconds: real): bool { return overlay.controller.seek(seconds); }
    function clear() { overlay.controller.reset(); }
    onPositionChanged: overlay.controller.advance(position)
    onPlayingChanged: overlay.controller.set_paused(!playing)
    Component.onCompleted: {
        overlay.controller.set_paused(!playing);
        overlay.controller.advance(position);
    }
}
