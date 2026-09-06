pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Item {
    id: danmakuLayer
    required property real fontSize
    required property real textOpacity
    required property real speed
    property bool titleOverlapsVideo: false
    property real titleBottomInVideo: 0
    clip: true
    // Only createObject-owned entries are destroyed here. Both ownership arrays
    // are cleared before destroy(), which Qt defers until the script returns.
    function receive(text) {
        if (!visible || text.length === 0 || liveEntries.length >= 64)
            return;
        const entry = danmakuComment.createObject(danmakuLayer, {
            "commentText": text
        });
        if (entry)
            liveEntries.push(entry);
    }
    property var liveEntries: []
    function clearComments() {
        const entries = liveEntries;
        liveEntries = [];
        laneEntries = [null, null, null, null, null, null, null, null];
        for (const entry of entries)
            entry.dispose();
    }
    onVisibleChanged: if (!visible)
        clearComments()
    property var laneEntries: [null, null, null, null, null, null, null, null]
    readonly property real laneTop: titleOverlapsVideo ? Math.max(40, Math.min(height * .4, titleBottomInVideo + 16)) : 40
    readonly property real laneSpacing: Math.max(30, Math.min(58, (height - laneTop - 50) / 8))
    function selectLane(entry, speed) {
        const startX = width + entry.implicitWidth;
        let earliestLane = 0;
        let earliestRight = Number.MAX_VALUE;
        for (let lane = 0; lane < laneEntries.length; ++lane) {
            const previous = laneEntries[lane];
            if (previous === null) {
                laneEntries[lane] = entry;
                return lane;
            }
            const previousRight = previous.x + previous.width;
            const gap = startX - previousRight;
            const catchesBeforeExit = speed > previous.motionSpeed && gap / (speed - previous.motionSpeed) < previousRight / previous.motionSpeed;
            if (gap >= 24 && !catchesBeforeExit) {
                laneEntries[lane] = entry;
                return lane;
            }
            if (previousRight < earliestRight) {
                earliestRight = previousRight;
                earliestLane = lane;
            }
        }
        laneEntries[earliestLane] = entry;
        return earliestLane;
    }
    function releaseLane(entry) {
        const index = liveEntries.indexOf(entry);
        if (index >= 0)
            liveEntries.splice(index, 1);
        if (entry.lane >= 0 && laneEntries[entry.lane] === entry)
            laneEntries[entry.lane] = null;
    }
    Component {
        id: danmakuComment
        Label {
            id: danmakuEntry
            required property string commentText
            property int lane: -1
            property real motionSpeed: 0
            text: commentText
            textFormat: Text.PlainText
            width: implicitWidth
            y: danmakuLayer.laneTop + lane * danmakuLayer.laneSpacing
            color: "#f4f5f3"
            opacity: danmakuLayer.textOpacity
            font.pixelSize: danmakuLayer.fontSize
            font.bold: true
            style: Text.Outline
            styleColor: "#d0000000"
            property bool disposed: false
            function dispose() {
                if (disposed)
                    return;
                disposed = true;
                danmakuMotion.stop();
                danmakuLayer.releaseLane(danmakuEntry);
                destroy();
            }
            Behavior on y {
                NumberAnimation {
                    duration: 180
                    easing.type: Easing.OutCubic
                }
            }
            NumberAnimation {
                id: danmakuMotion
                target: danmakuEntry
                property: "x"
                easing.type: Easing.Linear
                onFinished: danmakuEntry.dispose()
            }
            Component.onCompleted: {
                const textWidth = implicitWidth;
                const visibleDuration = 9000 / danmakuLayer.speed;
                const visibleDistance = danmakuLayer.width + textWidth;
                const pixelsPerMillisecond = visibleDistance / visibleDuration;
                motionSpeed = pixelsPerMillisecond;
                lane = danmakuLayer.selectLane(danmakuEntry, motionSpeed);
                danmakuMotion.from = danmakuLayer.width + textWidth;
                danmakuMotion.to = -textWidth;
                danmakuMotion.duration = Math.round((danmakuMotion.from - danmakuMotion.to) / pixelsPerMillisecond);
                x = danmakuMotion.from;
                danmakuMotion.start();
            }
        }
    }
}
