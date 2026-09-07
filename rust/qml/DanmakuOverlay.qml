pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import MinimalViewer

Item {
    id: layer
    required property real fontSize
    required property real textOpacity
    required property real speed
    property bool fullScreen: false
    property bool paused: false
    property bool titleOverlapsVideo: false
    property real titleBottomInVideo: 0
    property bool controlsOverlapVideo: false
    property real controlsTopInVideo: 0
    readonly property alias controller: backend
    readonly property int activeCount: backend.active_count
    readonly property int laneCount: backend.lane_count
    // Visual handles only. Comment data, lanes, clocks and lifetime live in Rust.
    property var visuals: ({})
    readonly property int visualCount: flowRows.children.length + topRows.children.length + bottomRows.children.length
    clip: true

    FontMetrics { id: metrics; font.pixelSize: layer.fontSize; font.bold: true }
    TextMetrics { id: measure; font: metrics.font }
    function configure() {
        backend.configure(width, height, metrics.height, fontSize, titleOverlapsVideo, titleBottomInVideo, controlsOverlapVideo, controlsTopInVideo, fullScreen, speed);
    }
    function receive(text: string, position: string, color: int): bool {
        return visible && backend.receive(text, position, color);
    }
    function clearComments() { backend.clear(); }
    function removeVisual(token: real) {
        const item = visuals[token];
        if (item) {
            delete visuals[token];
            item.destroy();
        }
    }
    function clearVisuals() {
        const old = visuals;
        visuals = ({});
        for (const token in old)
            old[token].destroy();
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
        function onHeightChanged() { layer.configure(); }
    }
    DanmakuController {
        id: backend
        onMeasure_requested: function(token, text) {
            measure.text = text;
            backend.measured(token, Math.ceil(measure.advanceWidth) + 2);
        }
        onSpawned: function(token, kind, text, color, width, from_x, to_x, y, duration) {
            const parentItem = [flowRows, topRows, bottomRows][kind];
            const item = label.createObject(parentItem, {
                "text": text, "color": color, "width": width, "x": from_x,
                "y": y, "destination": to_x, "duration": duration
            });
            if (item)
                layer.visuals[token] = item;
        }
        onRemoved: function(token) { layer.removeVisual(token); }
        onCleared: layer.clearVisuals()
    }
    // A render-frame notification, not a scheduler: Rust reads its own monotonic
    // clock and expires entries. No timer or callbacks run when the view is idle.
    FrameAnimation {
        running: layer.visible && backend.active_count > 0 && !backend.paused
        onTriggered: backend.tick()
    }
    // All labels of a kind share one animated origin. A newly created label
    // therefore follows the same vertical transition as existing comments.
    Item {
        id: flowRows
        width: layer.width
        y: backend.flow_origin
        Behavior on y { enabled: backend.active_count > 0; NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
    }
    Item {
        id: topRows
        width: layer.width
        y: backend.top_origin
        Behavior on y { enabled: backend.active_count > 0; NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
    }
    Item {
        id: bottomRows
        width: layer.width
        y: backend.bottom_origin
        Behavior on y { enabled: backend.active_count > 0; NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
    }
    Component {
        id: label
        Label {
            id: entry
            required property real destination
            required property int duration
            textFormat: Text.PlainText
            wrapMode: Text.NoWrap
            opacity: layer.textOpacity
            font: metrics.font
            style: Text.Outline
            styleColor: "#d0000000"
            Component.onCompleted: motion.start()
            NumberAnimation {
                id: motion
                target: entry
                property: "x"
                to: entry.destination
                duration: entry.duration
                easing.type: Easing.Linear
                paused: backend.paused
            }
        }
    }
}
