pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import MinimalViewer

Item {
    id: overlay
    enum Placement { Scrolling, Top, Bottom }
    required property real fontSize
    required property real textOpacity
    required property real speed
    property bool shadowEnabled: true
    property bool fullScreen: false
    property bool paused: false
    property var playbackClock: null
    property bool titleOverlapsVideo: false
    property real titleBottomInVideo: 0
    property bool controlsOverlapVideo: false
    property real controlsTopInVideo: 0
    readonly property alias controller: backend
    readonly property int activeCount: backend.active_count
    readonly property int laneCount: backend.lane_count
    // Visual handles only. Comment data, lanes, clocks and lifetime live in Rust.
    // A Map keeps storage tied to live entries, not the highest numeric token.
    property var visuals: new Map()
    readonly property int visualCount: flowRows.children.length + topRows.children.length + bottomRows.children.length
    clip: true

    FontMetrics {
        id: metrics
        // Match the application's Japanese font for both measurement and text.
        font.family: "Noto Sans CJK JP"
        font.pixelSize: overlay.fontSize
        font.bold: true
    }
    TextMetrics { id: measure; font: metrics.font }
    function configure() {
        backend.configure(width, height, metrics.height, fontSize, titleOverlapsVideo, titleBottomInVideo, controlsOverlapVideo, controlsTopInVideo, fullScreen, speed);
    }
    function receive(text, position, color, own = false): bool {
        return visible && backend.receive(text, position, color, own);
    }
    function clearComments() { backend.clear(); }
    function removeVisual(token: real) {
        const item = visuals.get(token);
        if (item) {
            visuals.delete(token);
            item.destroy();
        }
    }
    function clearVisuals() {
        const old = visuals;
        visuals = new Map();
        old.forEach(item => item.destroy());
    }
    onWidthChanged: configure()
    onHeightChanged: configure()
    onFontSizeChanged: configure()
    onTitleOverlapsVideoChanged: configure()
    onTitleBottomInVideoChanged: configure()
    onControlsOverlapVideoChanged: configure()
    onControlsTopInVideoChanged: configure()
    onFullScreenChanged: configure()
    onSpeedChanged: configure()
    onPausedChanged: backend.set_paused(paused)
    onVisibleChanged: backend.set_visible(visible)
    Component.onCompleted: {
        backend.set_visible(visible);
        configure();
        backend.set_paused(paused);
    }
    Connections {
        target: metrics
        function onHeightChanged() { overlay.configure(); }
    }
    DanmakuController {
        id: backend
        onMeasure_requested: function(token, text, own) {
            measure.text = text;
            backend.measured(token, Math.ceil(measure.advanceWidth) + 2 + (own ? 6 : 0));
        }
        onSpawned: function(token, kind, text, color, width, from_x, to_x, y, duration, own) {
            const parentItem = [flowRows, topRows, bottomRows][kind];
            const item = label.createObject(parentItem, {
                "text": text, "color": color, "width": width, "startX": from_x, "placement": kind,
                "y": y, "destination": to_x, "duration": duration, "own": own
            });
            if (item)
                overlay.visuals.set(token, item);
        }
        onPositioned: function(token, x) {
            const item = overlay.visuals.get(token);
            if (item && item.placement === DanmakuOverlay.Scrolling) item.x = x;
        }
        onRemoved: function(token) { overlay.removeVisual(token); }
        onRemeasure_requested: function(round, token) {
            const item = overlay.visuals.get(token);
            if (item) {
                measure.text = item.text;
                backend.remeasured(round, token, Math.ceil(measure.advanceWidth) + 2 + (item.own ? 6 : 0));
            } else {
                backend.remeasured(round, token, 0);
            }
        }
        onRelaid_out: function(token, width, from_x, to_x, y, duration) {
            const item = overlay.visuals.get(token);
            if (item)
                item.relayout(width, from_x, to_x, y, duration);
        }
        onCleared: overlay.clearVisuals()
    }
    // Rust advances from the media clock in playback mode and a monotonic clock
    // in direct reception mode. Playback keeps checking for upcoming comments.
    FrameAnimation {
        running: overlay.visible && (backend.active_count > 0 || backend.media_driven) && !backend.paused
        onTriggered: {
            if (backend.media_driven && overlay.playbackClock) {
                const position = overlay.playbackClock.commentary_position();
                if (position >= 0) backend.advance(position);
            } else backend.tick();
        }
    }
    // All labels of a kind share one animated origin. A newly created label
    // therefore follows the same vertical transition as existing comments.
    Item {
        id: flowRows
        width: overlay.width
        y: backend.flow_origin
        Behavior on y { enabled: backend.active_count > 0; NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
    }
    Item {
        id: topRows
        width: overlay.width
        y: backend.top_origin
        Behavior on y { enabled: backend.active_count > 0; NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
    }
    Item {
        id: bottomRows
        width: overlay.width
        y: backend.bottom_origin
        Behavior on y { enabled: backend.active_count > 0; NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
    }
    Component {
        id: label
        Label {
            id: entry
            required property int placement
            required property real startX
            required property real destination
            required property int duration
            required property bool own
            function relayout(newWidth, fromX, toX, newY, remaining) {
                motion.stop();
                width = newWidth;
                startX = fromX;
                destination = toX;
                y = newY;
                duration = remaining;
                x = Qt.binding(() => entry.placement === DanmakuOverlay.Scrolling ? entry.startX : (overlay.width - entry.width) / 2);
                if (placement === DanmakuOverlay.Scrolling && !backend.media_driven)
                    motion.start();
            }
            // Only scrolling labels animate x. Fixed labels retain this binding
            // to the new viewport center when the window or sidebar resizes it.
            x: placement === DanmakuOverlay.Scrolling ? startX : (overlay.width - width) / 2
            textFormat: Text.PlainText
            wrapMode: Text.NoWrap
            opacity: overlay.textOpacity
            font: metrics.font
            renderType: Text.QtRendering
            style: Text.Outline
            styleColor: "#d0000000"
            leftPadding: own ? 3 : 0
            rightPadding: own ? 3 : 0
            topPadding: own ? 1 : 0
            bottomPadding: own ? 1 : 0
            Loader {
                id: shadow
                objectName: "commentShadow"
                active: overlay.shadowEnabled
                visible: active
                x: entry.leftPadding + entry.font.pixelSize / 12
                y: entry.topPadding + entry.font.pixelSize / 12
                z: -1
                sourceComponent: DanmakuShadow {
                    text: entry.text
                    font: entry.font
                    viewportWidth: overlay.width
                    textX: entry.x + shadow.x
                }
            }
            background: Rectangle {
                visible: entry.own
                color: "transparent"
                border.color: "#ffe066"
                border.width: 1
            }
            Component.onCompleted: {
                if (placement === DanmakuOverlay.Scrolling && !backend.media_driven)
                    motion.start();
            }
            NumberAnimation {
                id: motion
                target: entry
                property: "x"
                to: entry.destination
                duration: entry.duration
                easing.type: Easing.Linear
                paused: running && backend.paused
            }
        }
    }
}
