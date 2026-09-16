pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
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
            text: root.timeLabel(slider.pressed ? slider.value : root.backend.position_ms)
                + " / " + root.timeLabel(root.backend.duration_ms)
            color: "#f4f5f3"
            font.pixelSize: 12
            font.family: "monospace"
        }
        Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignRight
            text: root.backend.transport_error || (root.backend.seeking ? qsTranslate("Viewer", "Seeking…")
                : root.backend.ended ? qsTranslate("Viewer", "Playback finished")
                : root.backend.paused ? qsTranslate("Viewer", "Paused") : "")
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
        enabled: !root.closing && root.backend.seekable && root.backend.duration_ms > 0
        Accessible.name: qsTranslate("Viewer", "Playback position")
        from: 0
        to: Math.max(1, root.backend.duration_ms)
        stepSize: 1000
        property var pendingTarget: null
        onEnabledChanged: if (!enabled) pendingTarget = null
        onMoved: {
            pendingTarget = value;
            if (!pressed) {
                root.backend.seek_to(pendingTarget);
                pendingTarget = null;
            }
        }
        onPressedChanged: {
            if (!pressed && pendingTarget !== null) {
                if (enabled) root.backend.seek_to(pendingTarget);
                pendingTarget = null;
            }
        }
        Binding {
            target: slider
            property: "value"
            value: Math.max(0, Math.min(slider.to, root.backend.position_ms))
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
            text: root.timeLabel(slider.pressed ? slider.value : target)
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
