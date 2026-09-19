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
    enum ProgramRole { Other, Watching, Broadcasting }
    readonly property bool pressed: slider.pressed
    readonly property bool hovered: seekHover.hovered
    readonly property real millisecondsPerSecond: 1000
    readonly property real secondsPerMinute: 60
    readonly property real minutesPerHour: 60
    readonly property real minimumTitleWidth: 90
    readonly property real titleInset: 4
    readonly property real titleHeight: 23
    readonly property real clockWidth: 48
    readonly property real clockHeight: 18
    readonly property real labelGap: 8
    readonly property real playheadSize: 16
    readonly property real boundaryHeight: 18
    readonly property real liveMarkerSize: 8
    readonly property color primaryColor: "#f4f5f3"
    readonly property color secondaryColor: "#b6bab6"
    spacing: 0

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
    function boundaryLabel(time) {
        if (!axis || !snapshot) return "--:--";
        let utc = null;
        if (time === axis.start) utc = axis.startUtc;
        else if (time === axis.end) utc = axis.endUtc;
        else {
            const preview = JSON.parse(backend.timeline_preview(snapshot.session, time) || "null");
            utc = preview ? preview.utc : null;
        }
        return utc !== null && utc !== undefined ? Qt.formatDateTime(new Date(utc), "hh:mm") : timeLabel(time);
    }
    function programRole(program) {
        if (viewingProgram && program.id === viewingProgram.id) return LiveTimeline.Watching;
        if (liveProgram && program.id === liveProgram.id) return LiveTimeline.Broadcasting;
        return LiveTimeline.Other;
    }
    function programLabel(program, role) {
        const title = program.title || qsTranslate("Viewer", "Program information unavailable");
        switch (role) {
        case LiveTimeline.Watching: return qsTranslate("Viewer", "Watching · %1").arg(title);
        case LiveTimeline.Broadcasting: return qsTranslate("Viewer", "On air · %1").arg(title);
        case LiveTimeline.Other: return title;
        }
    }
    function labelsOverlap(first, second) {
        return first.x < second.x + second.width + labelGap && second.x < first.x + first.width + labelGap;
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
        visible: (root.viewing !== null && root.viewing.offscreen) || status.text.length > 0
        spacing: root.labelGap
        Label {
            id: expiredMarker
            objectName: "expiredPlayheadMarker"
            Layout.fillWidth: true
            visible: root.viewing !== null && root.viewing.offscreen
            text: root.viewing ? (slider.mirrored ? "▶ " : "◀ ") + root.clockLabel(root.viewing.utc, root.viewing.position)
                + " · " + (root.viewingProgram ? root.viewingProgram.title : qsTranslate("Viewer", "Program information unavailable")) : ""
            textFormat: Text.PlainText
            elide: Text.ElideRight
            font.pixelSize: 11
            color: root.primaryColor
        }
        Label {
            id: status
            objectName: "liveTimelineStatus"
            text: root.backend.transport_error || (root.snapshot && root.snapshot.state === "seeking" ? qsTranslate("Viewer", "Seeking…")
                : root.snapshot && root.snapshot.state === "paused" ? (root.viewing && root.viewing.availability === "expired"
                    ? qsTranslate("Viewer", "Paused outside retained history") : qsTranslate("Backend", "Paused")) : "")
            textFormat: Text.PlainText; elide: Text.ElideRight
            Layout.fillWidth: true
            Layout.maximumWidth: expiredMarker.visible ? root.width / 2 : root.width
            horizontalAlignment: Text.AlignRight
            font.pixelSize: 11; color: root.backend.transport_error ? "#ffb4ab" : root.secondaryColor
        }
    }
    Item {
        id: titles
        Layout.fillWidth: true
        implicitHeight: root.titleHeight + root.titleInset
        Repeater {
            objectName: "programSegments"
            model: root.snapshot ? root.snapshot.programs : []
            delegate: Label {
                required property var modelData
                readonly property int role: root.programRole(modelData)
                readonly property real spanWidth: Math.max(0, root.fraction(modelData.end) - root.fraction(modelData.start)) * track.width
                objectName: "programSegmentLabel"
                visible: spanWidth >= root.minimumTitleWidth
                x: track.startX(modelData.start, modelData.end) + root.titleInset
                y: root.titleInset
                width: Math.max(0, spanWidth - root.titleInset * 2)
                height: root.titleHeight
                text: root.programLabel(modelData, role)
                textFormat: Text.PlainText
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
                horizontalAlignment: slider.mirrored ? Text.AlignRight : Text.AlignLeft
                font.pixelSize: 12
                font.weight: role === LiveTimeline.Watching ? Font.DemiBold : Font.Normal
                color: role === LiveTimeline.Watching ? root.primaryColor : root.secondaryColor
            }
        }
        Label {
            objectName: "unknownProgramLabel"
            visible: root.snapshot !== null && root.snapshot.programs.length === 0
            x: root.titleInset; y: root.titleInset
            width: parent.width - root.titleInset * 2; height: root.titleHeight
            text: qsTranslate("Viewer", "Program information unavailable")
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
            font.pixelSize: 12; color: root.secondaryColor
        }
    }
    ThemedSlider {
        id: slider
        objectName: "liveSeekSlider"
        Layout.fillWidth: true
        implicitHeight: 24
        padding: 0
        // Align the track endpoints with the 24px player margins; handles straddle them.
        leftPadding: -handle.width / 2
        rightPadding: -handle.width / 2
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
                width: parent.width; height: 4; radius: 2; color: "#9468716b"
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
                height: 3; radius: height / 2; color: root.primaryColor
            }
            Repeater {
                objectName: "programBoundaryMarkers"
                model: root.snapshot ? root.snapshot.boundaries : []
                delegate: Rectangle {
                    required property real modelData
                    objectName: "programBoundaryTick"
                    x: track.position(modelData) - width / 2
                    anchors.verticalCenter: parent.verticalCenter; anchors.alignWhenCentered: false
                    width: 1; height: root.boundaryHeight; color: "#7af4f5f3"
                }
            }
            Rectangle {
                objectName: "livePositionMarker"
                visible: root.snapshot !== null
                x: root.snapshot ? track.position(root.snapshot.live.position) - width / 2 : 0
                anchors.verticalCenter: parent.verticalCenter; anchors.alignWhenCentered: false
                width: root.liveMarkerSize; height: width; rotation: 45; color: "#d2e2d5"
            }
            Rectangle {
                objectName: "livePlayhead"
                visible: root.viewing !== null && !root.viewing.offscreen
                x: root.viewing ? track.position(root.viewing.position) - width / 2 : 0
                anchors.verticalCenter: parent.verticalCenter; anchors.alignWhenCentered: false
                width: root.playheadSize; height: width; radius: width / 2; color: root.primaryColor
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
            implicitWidth: root.playheadSize; implicitHeight: root.playheadSize; radius: width / 2
            color: "transparent"
            border.width: slider.pressed || slider.visualFocus ? 2 : 0
            border.color: root.primaryColor
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
    Item {
        id: clocks
        Layout.fillWidth: true
        implicitHeight: 20
        readonly property var boundaries: root.axis && root.snapshot
            ? [root.axis.start].concat(root.snapshot.boundaries.filter(time => time > root.axis.start && time < root.axis.end)) : []
        Repeater {
            objectName: "programBoundaryClocks"
            model: clocks.boundaries
            delegate: Label {
                required property real modelData
                required property int index
                readonly property real nextPosition: root.axis ? track.position(index + 1 < clocks.boundaries.length
                    ? clocks.boundaries[index + 1] : root.axis.end) : 0
                objectName: "programBoundaryClock"
                x: Math.max(0, Math.min(clocks.width - width, track.position(modelData) - (slider.mirrored ? width : 0)))
                width: Math.max(root.clockWidth, implicitWidth); height: root.clockHeight
                visible: (!liveLabel.visible || !root.labelsOverlap(this, liveLabel))
                    && (!endLabel.visible || !root.labelsOverlap(this, endLabel))
                    && Math.abs(nextPosition - track.position(modelData)) >= width + root.labelGap
                text: root.boundaryLabel(modelData)
                verticalAlignment: Text.AlignVCenter
                horizontalAlignment: slider.mirrored ? Text.AlignRight : Text.AlignLeft
                font.pixelSize: 10; color: root.secondaryColor
            }
        }
        Label {
            id: liveLabel
            objectName: "livePositionLabel"
            visible: root.snapshot !== null
            text: "LIVE" + (root.snapshot && root.snapshot.live.utc !== null
                ? " " + Qt.formatDateTime(new Date(root.snapshot.live.utc), "hh:mm") : "")
            x: root.snapshot ? Math.max(0, Math.min(parent.width - width, track.x + track.position(root.snapshot.live.position) - width / 2)) : 0
            height: root.clockHeight
            verticalAlignment: Text.AlignVCenter
            font.pixelSize: 10; font.bold: true; color: "#d2e2d5"
        }
        Label {
            id: endLabel
            objectName: "timelineEndLabel"
            x: slider.mirrored ? 0 : parent.width - width
            width: Math.max(root.clockWidth, implicitWidth); height: root.clockHeight
            visible: root.axis !== null && (!liveLabel.visible || !root.labelsOverlap(this, liveLabel))
            text: root.axis ? root.boundaryLabel(root.axis.end) : "--:--"
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: slider.mirrored ? Text.AlignLeft : Text.AlignRight
            font.pixelSize: 10; color: "#939d94"
        }
    }
}
