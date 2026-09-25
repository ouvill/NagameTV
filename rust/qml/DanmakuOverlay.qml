pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import MinimalViewer

Item {
    id: overlay
    enum Placement { Scrolling, Top, Bottom, Pop }
    required property real fontSize
    required property real textOpacity
    required property real speed
    property string displayMode: "scroll"
    property string placementMode: "sequential"
    property string densityMode: "normal"
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
    readonly property bool animating: frames.running
    // Visual handles only. Comment data, lanes, clocks and lifetime live in Rust.
    // A Map keeps storage tied to live entries, not the highest numeric token.
    property var visuals: new Map()
    readonly property int visualCount: flowRows.children.length + topRows.children.length + bottomRows.children.length + popRows.children.length
    clip: true
    ScreenshotText { id: captureText }
    function screenshotLayer(target) {
        const origin = mapToItem(target, 0, 0);
        const commands = [];
        for (const row of [flowRows, topRows, bottomRows, popRows]) {
            for (const entry of row.children) {
                if (!entry.visible) continue;
                const position = row.mapToItem(overlay, entry.x, entry.y);
                const pose = {x: position.x + entry.width / 2, y: position.y + entry.height / 2, degrees: entry.rotation};
                if (entry.own) {
                    const color = Qt.rgba(1, 224 / 255, 102 / 255, entry.opacity).toString();
                    for (const rect of [[0, 0, entry.width, 1], [0, entry.height - 1, entry.width, 1],
                        [0, 1, 1, entry.height - 2], [entry.width - 1, 1, 1, entry.height - 2]])
                        commands.push({kind: "rect", x: position.x + rect[0], y: position.y + rect[1],
                            width: rect[2], height: rect[3], color: color, pose: pose});
                }
                const command = captureText.command(entry, position.x + entry.leftPadding, position.y,
                    1, entry.styleColor.toString(), 2, entry.captureShadow, false, false);
                command.pose = pose;
                commands.push(command);
            }
        }
        return {x: origin.x, y: origin.y, width: width, height: height, commands: commands};
    }

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
            item.visible = false;
            item.destroy();
        }
    }
    function clearVisuals() {
        const old = visuals;
        visuals = new Map();
        // QML destruction is deferred. Hide retired labels immediately so a
        // seek/mode change, or a capture in the same event turn, cannot include
        // the old generation alongside its replacement.
        old.forEach(item => { item.visible = false; item.destroy(); });
    }
    onDensityModeChanged: backend.set_density(densityMode)
    onDisplayModeChanged: backend.set_presentation(displayMode, placementMode)
    onPlacementModeChanged: backend.set_presentation(displayMode, placementMode)
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
        backend.set_density(densityMode);
        backend.set_presentation(displayMode, placementMode);
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
        onSpawned: function(token, kind, text, color, width, from_x, to_x, y, duration, own, rotation, opacity) {
            const parentItem = [flowRows, topRows, bottomRows, popRows][kind];
            const item = label.createObject(parentItem, {
                "text": text, "color": color, "width": width, "startX": from_x, "placement": kind,
                "y": y, "destination": to_x, "duration": duration, "own": own,
                "rotation": rotation, "motionOpacity": opacity
            });
            if (item)
                overlay.visuals.set(token, item);
        }
        onPositioned: function(token, x, y, rotation, opacity) {
            const item = overlay.visuals.get(token);
            if (item) { item.x = x; item.y = y; item.rotation = rotation; item.motionOpacity = opacity; }
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
        id: frames
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
        Behavior on y { enabled: backend.active_count > 0; NumberAnimation { duration: Theme.moveDuration; easing.type: Easing.OutCubic } }
    }
    Item {
        id: topRows
        width: overlay.width
        y: backend.top_origin
        Behavior on y { enabled: backend.active_count > 0; NumberAnimation { duration: Theme.moveDuration; easing.type: Easing.OutCubic } }
    }
    Item {
        id: bottomRows
        width: overlay.width
        y: backend.bottom_origin
        Behavior on y { enabled: backend.active_count > 0; NumberAnimation { duration: Theme.moveDuration; easing.type: Easing.OutCubic } }
    }
    Item { id: popRows; width: overlay.width; height: overlay.height }
    Component {
        id: label
        Label {
            id: entry
            required property int placement
            required property real startX
            required property real destination
            required property int duration
            required property bool own
            property real motionOpacity: 1
            // Inverse-project the video rectangle onto the label's x axis.
            // Rotated, very long text needs a wider crop, but its shadow texture
            // must remain bounded by the viewport, never the whole string.
            readonly property real tiltCos: Math.cos(rotation * Math.PI / 180)
            readonly property real tiltSin: Math.sin(rotation * Math.PI / 180)
            readonly property real shadowViewportWidth: overlay.width * Math.abs(tiltCos) + overlay.height * Math.abs(tiltSin)
            readonly property real shadowViewportLeft: -(x + width / 2) * tiltCos - (y + height / 2) * tiltSin
                + width / 2 + Math.min(0, overlay.width * tiltCos) + Math.min(0, overlay.height * tiltSin)
            readonly property var captureShadow: shadow.active && shadow.view
                ? {offset: shadow.x - leftPadding, radius: shadow.view.blurRadius} : null
            function relayout(newWidth, fromX, toX, newY, remaining) {
                width = newWidth;
                startX = fromX;
                destination = toX;
                y = newY;
                duration = remaining;
                x = fromX;
            }
            // Rust owns every trajectory, for both media and direct reception.
            x: startX
            textFormat: Text.PlainText
            wrapMode: Text.NoWrap
            opacity: overlay.textOpacity * motionOpacity
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
                readonly property DanmakuShadow view: item as DanmakuShadow
                objectName: "commentShadow"
                active: overlay.shadowEnabled
                visible: active
                x: entry.leftPadding + entry.font.pixelSize / 12
                y: entry.topPadding + entry.font.pixelSize / 12
                z: -1
                sourceComponent: DanmakuShadow {
                    text: entry.text
                    font: entry.font
                    viewportWidth: entry.shadowViewportWidth
                    textX: -entry.shadowViewportLeft + shadow.x
                }
            }
            background: Rectangle {
                visible: entry.own
                color: "transparent"
                border.color: "#ffe066"
                border.width: 1
            }
        }
    }
}
