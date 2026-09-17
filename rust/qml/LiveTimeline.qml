pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    property bool closing: false
    readonly property var snapshot: JSON.parse(backend.live_timeline || "null")
    readonly property var axis: slider.gesture ? slider.gesture.axis : snapshot ? snapshot.axis : null
    readonly property var viewing: snapshot ? snapshot.viewing : null
    readonly property var viewingProgram: viewing ? viewing.program : null
    readonly property var liveProgram: snapshot ? snapshot.live.program : null
    readonly property bool differentProgram: liveProgram !== null && (viewingProgram === null || viewingProgram.id !== liveProgram.id)
    readonly property bool pressed: slider.pressed
    readonly property bool hovered: seekHover.hovered
    readonly property real millisecondsPerSecond: 1000
    readonly property real secondsPerMinute: 60
    readonly property real minutesPerHour: 60
    readonly property real minimumTitleWidth: 90
    spacing: 2

    function fraction(time) {
        return axis ? Math.max(0, Math.min(1, (time - axis.start) / Math.max(1, axis.end - axis.start))) : 0;
    }
    function timeLabel(milliseconds) {
        if (milliseconds === null || !Number.isFinite(milliseconds) || milliseconds < 0) return "--:--";
        const total = Math.floor(milliseconds / millisecondsPerSecond);
        const hours = Math.floor(total / (secondsPerMinute * minutesPerHour));
        const minutes = Math.floor(total / secondsPerMinute) % minutesPerHour;
        const seconds = total % secondsPerMinute;
        return (hours > 0 ? hours + ":" + String(minutes).padStart(2, "0") : String(minutes))
            + ":" + String(seconds).padStart(2, "0");
    }
    function clockLabel(utc, media) {
        return utc !== null && utc !== undefined ? Qt.formatDateTime(new Date(utc), "hh:mm:ss") : timeLabel(media);
    }
    function available(model, time) {
        return model && model.available.some(span => time >= span.start && time < span.end);
    }
    function commit(time, model) {
        if (!closing && snapshot && model && model.session === snapshot.session && available(model, time))
            backend.seek_timeline(model.session, time);
    }
    function previewLabel(time) {
        if (!snapshot) return "";
        const result = JSON.parse(backend.timeline_preview(snapshot.session, time) || "null");
        if (!result) return "";
        return clockLabel(result.utc, time) + " · " + (result.title || qsTranslate("Viewer", "Program information unavailable"))
            + (result.available ? "" : " · " + qsTranslate("Viewer", "Outside retained history"));
    }

    RowLayout {
        Layout.fillWidth: true
        Label {
            objectName: "viewingProgramLabel"
            Layout.fillWidth: true
            text: qsTranslate("Viewer", "Watching: %1").arg(root.viewingProgram ? root.viewingProgram.title : qsTranslate("Viewer", "Program information unavailable"))
                + (root.viewingProgram && root.viewingProgram.duration !== null ? "  " + root.timeLabel(root.viewingProgram.elapsed) + " / " + root.timeLabel(root.viewingProgram.duration) : "")
            textFormat: Text.PlainText
            elide: Text.ElideRight
            font.pixelSize: 12
            color: "#f4f5f3"
        }
        Button {
            id: liveButton
            objectName: "returnToLiveButton"
            visible: root.snapshot !== null && root.snapshot.available.length > 0
            enabled: !root.closing && root.backend.seekable
            text: qsTranslate("Viewer", "Return to live") + "  ·  −" + root.timeLabel(root.snapshot && root.viewing ? Math.max(0, root.snapshot.live.position - root.viewing.position) : 0)
            implicitHeight: 28; leftPadding: 12; rightPadding: 12
            contentItem: Label { text: liveButton.text; color: "#d2e2d5"; font.pixelSize: 12; verticalAlignment: Text.AlignVCenter }
            background: Rectangle { radius: height / 2; color: liveButton.down ? "#429caf9f" : "#229caf9f"; border.color: liveButton.visualFocus ? "#f4f5f3" : "#629caf9f" }
            onClicked: root.backend.return_to_live()
        }
    }
    RowLayout {
        Layout.fillWidth: true
        Label {
            objectName: "broadcastProgramLabel"
            Layout.fillWidth: true
            text: root.differentProgram ? qsTranslate("Viewer", "On air: %1").arg(root.liveProgram.title) : ""
            textFormat: Text.PlainText; elide: Text.ElideRight
            font.pixelSize: 11; color: "#b6bab6"
        }
        Label {
            objectName: "liveTimelineStatus"
            text: root.backend.transport_error || (root.snapshot && root.snapshot.state === "seeking" ? qsTranslate("Viewer", "Seeking…")
                : root.snapshot && root.snapshot.state === "paused" ? (root.viewing && root.viewing.availability === "expired"
                    ? qsTranslate("Viewer", "Paused outside retained history") : qsTranslate("Backend", "Paused")) : "")
            textFormat: Text.PlainText; elide: Text.ElideRight
            Layout.maximumWidth: root.width / 2
            font.pixelSize: 11; color: root.backend.transport_error ? "#ffb4ab" : "#b6bab6"
        }
    }
    ThemedSlider {
        id: slider
        objectName: "liveSeekSlider"
        Layout.fillWidth: true
        implicitHeight: 48
        enabled: !root.closing && root.snapshot !== null && root.snapshot.available.length > 0 && root.backend.seekable
        Accessible.name: qsTranslate("Viewer", "Playback position")
        from: root.axis ? root.axis.start : 0
        to: root.axis ? root.axis.end : 1
        stepSize: root.millisecondsPerSecond
        property var gesture: null
        property var pendingTarget: null
        onEnabledChanged: if (!enabled) pendingTarget = null
        onMoved: {
            pendingTarget = value;
            if (!pressed) { root.commit(value, root.snapshot); pendingTarget = null; }
        }
        onPressedChanged: {
            if (pressed) gesture = root.snapshot;
            else {
                if (pendingTarget !== null && enabled) root.commit(pendingTarget, gesture);
                pendingTarget = null;
                gesture = null;
            }
        }
        Binding {
            target: slider; property: "value"
            value: root.viewing ? Math.max(slider.from, Math.min(slider.to, root.viewing.position)) : slider.from
            when: !slider.pressed
            restoreMode: Binding.RestoreNone
        }
        background: Item {
            id: track
            x: slider.leftPadding + slider.handle.width / 2
            y: slider.topPadding + slider.availableHeight / 2 - height / 2
            width: slider.availableWidth - slider.handle.width
            height: slider.pressed || root.hovered ? 10 : 8
            function position(time) { return (slider.mirrored ? 1 - root.fraction(time) : root.fraction(time)) * width; }
            function startX(start, end) { return slider.mirrored ? position(end) : position(start); }
            Rectangle {
                objectName: "programTrack"
                anchors.verticalCenter: parent.verticalCenter; anchors.alignWhenCentered: false
                width: parent.width; height: 4; radius: 2; color: "#42ffffff"
            }
            Repeater {
                objectName: "retainedRanges"
                model: root.snapshot ? root.snapshot.available : []
                delegate: Rectangle {
                    required property var modelData
                    objectName: "retainedRangeFill"
                    anchors.verticalCenter: parent.verticalCenter; anchors.alignWhenCentered: false
                    x: track.startX(modelData.start, modelData.end)
                    width: Math.max(0, root.fraction(modelData.end) - root.fraction(modelData.start)) * track.width
                    height: track.height; radius: height / 2; color: "#9caf9f"
                }
            }
            Rectangle {
                objectName: "programProgressFill"
                readonly property var span: root.liveProgram ? root.liveProgram.span : null
                visible: span !== null
                anchors.verticalCenter: parent.verticalCenter; anchors.alignWhenCentered: false
                x: span && root.snapshot ? track.startX(span.start, root.snapshot.live.position) : 0
                width: span && root.snapshot ? Math.max(0, root.fraction(Math.min(span.end, root.snapshot.live.position)) - root.fraction(span.start)) * track.width : 0
                height: 3; radius: height / 2; color: "#f4f5f3"
            }
            Repeater {
                objectName: "programSegments"
                model: root.snapshot ? root.snapshot.programs : []
                delegate: Item {
                    required property var modelData
                    x: track.startX(modelData.start, modelData.end)
                    width: Math.max(0, root.fraction(modelData.end) - root.fraction(modelData.start)) * track.width
                    height: track.height
                    Label {
                        visible: parent.width >= root.minimumTitleWidth
                        x: 5; y: parent.height + 4; width: parent.width - 10
                        text: modelData.title; textFormat: Text.PlainText; elide: Text.ElideRight
                        font.pixelSize: 10; color: "#a6aaa6"
                    }
                }
            }
            Repeater {
                objectName: "programBoundaryMarkers"
                model: root.snapshot ? root.snapshot.boundaries : []
                delegate: Rectangle {
                    required property real modelData
                    objectName: "programBoundaryTick"
                    x: track.position(modelData) - width / 2
                    anchors.verticalCenter: parent.verticalCenter; anchors.alignWhenCentered: false
                    width: 1; height: track.height + 4; color: "#66ffffff"
                }
            }
            Rectangle {
                objectName: "livePositionMarker"
                visible: root.snapshot !== null
                x: root.snapshot ? track.position(root.snapshot.live.position) - width / 2 : 0
                anchors.verticalCenter: parent.verticalCenter; anchors.alignWhenCentered: false
                width: 7; height: 7; rotation: 45; color: "#d2e2d5"
            }
            Label {
                visible: root.snapshot !== null
                text: "LIVE"; font.pixelSize: 9; font.bold: true; color: "#d2e2d5"
                x: root.snapshot ? Math.max(0, Math.min(track.width - width, track.position(root.snapshot.live.position) - width / 2)) : 0
                y: -height - 5
            }
            Rectangle {
                objectName: "livePlayhead"
                visible: root.viewing !== null && !root.viewing.offscreen
                x: root.viewing ? track.position(root.viewing.position) - width / 2 : 0
                anchors.verticalCenter: parent.verticalCenter; anchors.alignWhenCentered: false
                width: 12; height: 12; radius: width / 2; color: "#f4f5f3"
            }
            Label {
                objectName: "expiredPlayheadMarker"
                visible: root.viewing !== null && root.viewing.offscreen
                x: slider.mirrored ? track.width - width : 0
                y: -height - 5
                text: root.viewing ? (slider.mirrored ? "▶ " : "◀ ") + root.clockLabel(root.viewing.utc, root.viewing.position) : ""
                font.pixelSize: 11; color: "#f4f5f3"
            }
            Rectangle {
                objectName: "seekTargetMarker"
                visible: root.snapshot !== null && root.snapshot.seekTarget !== null
                x: visible ? track.position(root.snapshot.seekTarget) - width / 2 : 0
                anchors.verticalCenter: parent.verticalCenter; anchors.alignWhenCentered: false
                width: 14; height: 14; radius: width / 2; color: "transparent"; border.width: 2; border.color: "#9caf9f"
            }
        }
        handle: Rectangle {
            x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
            y: slider.topPadding + slider.availableHeight / 2 - height / 2
            implicitWidth: 12; implicitHeight: 12; radius: width / 2
            color: "transparent"
            border.width: slider.pressed || slider.visualFocus ? 2 : 0
            border.color: "#f4f5f3"
        }
        HoverHandler { id: seekHover }
        ToolTip {
            id: preview
            objectName: "liveSeekPreview"
            parent: slider
            readonly property real fraction: Math.max(0, Math.min(1,
                (seekHover.point.position.x - slider.leftPadding - slider.handle.width / 2) / Math.max(1, slider.availableWidth - slider.handle.width)))
            readonly property real target: slider.from + (slider.to - slider.from) * (slider.mirrored ? 1 - fraction : fraction)
            visible: root.visible && root.hovered && root.snapshot !== null
            text: root.previewLabel(slider.pressed ? slider.value : target)
            x: Math.max(0, Math.min(slider.width - implicitWidth, seekHover.point.position.x - implicitWidth / 2))
            y: -implicitHeight - 6; padding: 9
            contentItem: Label { text: preview.text; textFormat: Text.PlainText; color: "#f4f5f3"; font.pixelSize: 12 }
            background: Rectangle { radius: 8; color: "#e61b1d1b"; border.color: "#38ffffff" }
        }
    }
    RowLayout {
        Layout.fillWidth: true
        Label {
            text: root.axis ? root.clockLabel(root.axis.startUtc, root.axis.start) : "--:--"
            font.pixelSize: 10; color: "#a6aaa6"
        }
        Item { Layout.fillWidth: true }
        Label {
            text: root.axis ? root.clockLabel(root.axis.endUtc, root.axis.end) : "--:--"
            font.pixelSize: 10; color: "#a6aaa6"
        }
    }
}
