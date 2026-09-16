pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    property bool closing: false
    readonly property bool pressed: slider.pressed
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
            value: Math.max(0, root.backend.position_ms)
            when: !slider.pressed
            restoreMode: Binding.RestoreNone
        }
    }
}
