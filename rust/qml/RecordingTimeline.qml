pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    readonly property bool live: backend.timeshift === true
    readonly property bool recording: backend.recording === true
    readonly property bool transport: recording || live
    readonly property var program: JSON.parse(backend.current_program_data || "null")
    readonly property bool mappedProgram: !recording && program !== null
        && Number.isFinite(program.playbackStartMs) && Number.isFinite(program.playbackEndMs)
        && program.playbackEndMs > program.playbackStartMs
    readonly property real seekStart: Number.isFinite(backend.window_start_ms) ? backend.window_start_ms : 0
    readonly property real axisStart: mappedProgram ? (live ? Math.min(program.playbackStartMs, seekStart) : program.playbackStartMs) : transport ? seekStart : 0
    readonly property real axisEnd: mappedProgram ? (live ? Math.max(program.playbackEndMs, seekEnd) : program.playbackEndMs) : transport ? Math.max(1, seekEnd) : 1
    readonly property real progressStart: mappedProgram ? program.playbackStartMs : axisStart
    readonly property real programProgress: Math.max(0, Math.min(1, backend.program_progress || 0))
    readonly property real progressEnd: transport ? backend.position_ms
        : mappedProgram ? program.playbackStartMs + programProgress * (program.playbackEndMs - program.playbackStartMs) : programProgress
    readonly property real millisecondsPerSecond: 1000
    readonly property var programBoundaries: live && backend.seekable
        ? JSON.parse(backend.timeshift_program_boundaries || "[]").filter(time => time > seekStart && time < seekEnd) : []
    function fraction(time) { return Math.max(0, Math.min(1, (time - axisStart) / Math.max(1, axisEnd - axisStart))); }
    function available(time) { return backend.seekable && time >= seekStart && time <= seekEnd; }
    function seek(time) { if (available(time)) backend.seek_to(time); }
    function previewLabel(time) {
        const label = mappedProgram && Number.isFinite(program.startAt)
            ? Qt.formatDateTime(new Date(program.startAt + time - program.playbackStartMs), "hh:mm:ss") : timeLabel(time);
        return label + (available(time) ? "" : " · " + qsTranslate("Viewer", "Outside retained history"));
    }
    readonly property real seekEnd: Number.isFinite(backend.window_end_ms)
        ? backend.window_end_ms : backend.duration_ms
    property bool closing: false
    readonly property bool pressed: slider.pressed
    readonly property bool hovered: slider.enabled && seekHover.hovered
    spacing: 0

    function timeLabel(milliseconds) {
        if (milliseconds < 0 || !Number.isFinite(milliseconds)) return "--:--";
        const total = Math.floor(milliseconds / 1000);
        const hours = Math.floor(total / 3600);
        const minutes = Math.floor(total / 60) % 60;
        const seconds = total % 60;
        return (hours > 0 ? hours + ":" + String(minutes).padStart(2, "0") : String(minutes))
            + ":" + String(seconds).padStart(2, "0");
    }

    RowLayout {
        Layout.fillWidth: true
        Label {
            objectName: "recordingTime"
            text: root.recording ? root.timeLabel(slider.pressed ? slider.value : root.backend.position_ms)
                + " / " + (root.backend.duration_estimated === true ? "≈" : "") + root.timeLabel(root.backend.duration_ms)
                : root.program ? qsTranslate("Viewer", "%1% of program").arg(Math.round((root.backend.program_progress || 0) * 100))
                : root.transport ? root.timeLabel(root.backend.position_ms) : ""

            color: "#f4f5f3"
            font.pixelSize: 12
            font.family: "monospace"
        }
        Button {
            id: liveButton
            objectName: "returnToLiveButton"
            visible: root.live
            text: qsTranslate("Viewer", "Return to live") + "  ·  −" + root.timeLabel(root.backend.live_delay_ms)
            implicitHeight: 30; leftPadding: 12; rightPadding: 12
            contentItem: Label { text: liveButton.text; color: "#d2e2d5"; font.pixelSize: 12; verticalAlignment: Text.AlignVCenter }
            background: Rectangle { radius: height / 2; color: liveButton.down ? "#429caf9f" : "#229caf9f"; border.color: liveButton.visualFocus ? "#f4f5f3" : "#629caf9f" }
            enabled: !root.closing && root.backend.seekable
            onClicked: root.backend.return_to_live()
        }
        Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignRight
            text: root.backend.transport_error || (root.backend.seeking ? qsTranslate("Viewer", "Seeking…")
                : root.backend.ended ? qsTranslate("Backend", "Playback finished")
                : root.backend.paused ? qsTranslate("Backend", "Paused") : "")
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: root.backend.transport_error ? "#ffb4ab" : "#b6bab6"
            font.pixelSize: 12
        }
    }
    ThemedSlider {
        id: slider
        objectName: "recordingSeekSlider"
        Layout.fillWidth: true
        enabled: !root.closing && root.transport && root.backend.seekable && root.seekEnd > root.seekStart
        Accessible.name: qsTranslate("Viewer", "Playback position")
        from: root.axisStart
        to: root.axisEnd
        stepSize: root.millisecondsPerSecond
        background: Item {
            id: track
            x: slider.leftPadding + slider.handle.width / 2
            y: slider.topPadding + slider.availableHeight / 2 - height / 2
            width: slider.availableWidth - slider.handle.width
            height: slider.pressed || root.hovered ? 10 : 8
            function startX(start, end) { return (slider.mirrored ? 1 - root.fraction(end) : root.fraction(start)) * width; }
            Rectangle {
                objectName: "programTrack"
                anchors.verticalCenter: parent.verticalCenter
                anchors.alignWhenCentered: false
                width: parent.width; height: 4; radius: 2; color: "#42ffffff"
            }
            Rectangle {
                objectName: "retainedRangeFill"
                anchors.verticalCenter: parent.verticalCenter
                anchors.alignWhenCentered: false
                visible: root.live && root.backend.seekable
                x: parent.startX(root.seekStart, root.seekEnd)
                width: (root.fraction(root.seekEnd) - root.fraction(root.seekStart)) * parent.width
                height: parent.height; radius: height / 2
                color: "#9caf9f"
            }
            Rectangle {
                objectName: "programProgressFill"
                x: parent.startX(root.progressStart, root.progressEnd)
                width: Math.max(0, root.fraction(root.progressEnd) - root.fraction(root.progressStart)) * parent.width
                anchors.verticalCenter: parent.verticalCenter
                anchors.alignWhenCentered: false
                height: 3; radius: height / 2; color: "#f4f5f3"
            }
            Repeater {
                objectName: "programBoundaryMarkers"
                model: root.programBoundaries
                delegate: Rectangle {
                    required property real modelData
                    objectName: "programBoundaryTick"
                    x: track.startX(modelData, modelData) - width / 2
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.alignWhenCentered: false
                    width: 1; height: track.height + 4
                    color: "#66ffffff"
                }
            }
        }
        handle: Rectangle {
            x: slider.leftPadding + slider.visualPosition * (slider.availableWidth - width)
            y: slider.topPadding + slider.availableHeight / 2 - height / 2
            implicitWidth: 12; implicitHeight: 12; radius: width / 2
            visible: root.transport && root.backend.seekable
            color: "#f4f5f3"
            scale: slider.pressed || root.hovered || slider.visualFocus ? 1.2 : 1
            border.width: slider.visualFocus ? 2 : 0; border.color: "#9caf9f"
        }
        property var pendingTarget: null
        onEnabledChanged: if (!enabled) pendingTarget = null
        onMoved: {
            pendingTarget = value;
            if (!pressed) {
                root.seek(pendingTarget);
                pendingTarget = null;
            }
        }
        onPressedChanged: {
            if (!pressed && pendingTarget !== null) {
                if (enabled) root.seek(pendingTarget);
                pendingTarget = null;
            }
        }
        Binding {
            target: slider
            property: "value"
            value: Math.max(slider.from, Math.min(slider.to, root.progressEnd))
            when: !slider.pressed
            restoreMode: Binding.RestoreNone
        }
        HoverHandler { id: seekHover }
        ToolTip {
            id: preview
            objectName: "recordingSeekPreview"
            parent: slider
            // Match the thumb's travel, including padding and its half-width.
            readonly property real fraction: Math.max(0, Math.min(1,
                (seekHover.point.position.x - slider.leftPadding - slider.handle.width / 2)
                / Math.max(1, slider.availableWidth - slider.handle.width)))
            readonly property real target: slider.from + (slider.to - slider.from)
                * (slider.mirrored ? 1 - fraction : fraction)
            visible: root.visible && root.hovered
            text: root.previewLabel(slider.pressed ? slider.value : target)
            x: Math.max(0, Math.min(slider.width - implicitWidth,
                seekHover.point.position.x - implicitWidth / 2))
            y: -implicitHeight - 6
            padding: 9
            contentItem: Label {
                text: preview.text
                color: "#f4f5f3"
                font.pixelSize: 12
                font.family: "monospace"
            }
            background: Rectangle {
                radius: 8
                color: "#e61b1d1b"
                border.color: "#38ffffff"
            }
        }
    }
}
