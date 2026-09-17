pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    readonly property var saved: JSON.parse(backend.timeshift_limits)
    property string storageValue: backend.timeshift_storage === "filesystem" ? "filesystem" : "memory"
    readonly property real mibBytes: 1024 * 1024
    readonly property real bitsPerByte: 8
    readonly property real assumedMegabitsPerSecond: 20
    readonly property real megabitBits: 1000000
    readonly property real secondsPerMinute: 60
    readonly property bool measured: Number.isFinite(backend.timeshift_bytes_per_second) && backend.timeshift_bytes_per_second > 0
    readonly property real bytesPerSecond: measured ? backend.timeshift_bytes_per_second : assumedMegabitsPerSecond * megabitBits / bitsPerByte
    readonly property real capacityMiB: storageValue === "memory" ? memory.value : files.value
    readonly property real estimateSeconds: Math.min(capacityMiB * mibBytes / bytesPerSecond, minutes.value * secondsPerMinute)
    readonly property string estimateTime: qsTranslate("Viewer", "%1 min %2 sec")
        .arg(Math.floor(estimateSeconds / secondsPerMinute)).arg(Math.floor(estimateSeconds % secondsPerMinute))
    spacing: 24

    SettingsToggle {
        id: enabledToggle
        objectName: "timeshiftEnabled"
        Layout.fillWidth: true
        text: qsTranslate("Viewer", "Enable timeshift")
        description: qsTranslate("Viewer", "Pause and rewind live TV.")
        checked: root.backend.timeshift_storage !== "off"
    }
    ColumnLayout {
        Layout.fillWidth: true; spacing: 12
        Label { text: qsTranslate("Viewer", "Storage"); color: "#f4f5f3"; font.pixelSize: 18 }
        SegmentedControl {
            id: storage
            objectName: "timeshiftStorage"
            objectNamePrefix: "timeshift-"
            Layout.fillWidth: true; Layout.maximumWidth: 400
            enabled: enabledToggle.checked
            options: [{value: "memory", label: qsTranslate("Viewer", "Memory")},
                {value: "filesystem", label: qsTranslate("Viewer", "Temporary files")}]
            value: root.storageValue
            onSelected: function(value) { root.storageValue = value; }
        }
        Label {
            objectName: "timeshiftStorageDescription"
            Layout.fillWidth: true; wrapMode: Text.Wrap
            text: root.storageValue === "memory"
                ? qsTranslate("Viewer", "Memory keeps rewinding quick without writing to storage. Choose a limit that leaves room for your other apps.")
                : qsTranslate("Viewer", "Temporary files keep longer history with less RAM. They use storage space and continuous disk writes, and are deleted when playback stops.")
            color: "#b6bab6"; font.pixelSize: 14
        }
    }
    Rectangle {
        Layout.fillWidth: true
        implicitHeight: budget.implicitHeight + 40
        radius: 14; color: "#1c201d"; border.color: "#343c35"
        ColumnLayout {
            id: budget
            anchors { left: parent.left; right: parent.right; top: parent.top; margins: 20 }
            spacing: 16
            Label {
                Layout.fillWidth: true; wrapMode: Text.Wrap
                text: root.storageValue === "memory" ? qsTranslate("Viewer", "Maximum TS memory") : qsTranslate("Viewer", "Maximum temporary files")
                color: "#f4f5f3"; font.pixelSize: 18
            }
            RowLayout {
                spacing: 12
                ThemedSpinBox {
                    id: memory; objectName: "timeshiftMemoryLimit"
                    visible: root.storageValue === "memory"; enabled: enabledToggle.checked
                    from: root.saved.min_mib; to: root.saved.max_mib; value: root.saved.memory_mib
                    Accessible.name: qsTranslate("Viewer", "Maximum TS memory (MiB)")
                }
                ThemedSpinBox {
                    id: files; objectName: "timeshiftFileLimit"
                    visible: root.storageValue === "filesystem"; enabled: enabledToggle.checked
                    from: root.saved.min_mib; to: root.saved.max_mib; value: root.saved.filesystem_mib
                    Accessible.name: qsTranslate("Viewer", "Maximum temporary files (MiB)")
                }
                Label { text: "MiB"; color: "#b6bab6"; font.pixelSize: 14 }
            }
            Label {
                objectName: "timeshiftEstimate"
                Layout.fillWidth: true; wrapMode: Text.Wrap
                text: qsTranslate("Viewer", "About %1 of history").arg(root.estimateTime)
                color: "#bcd3c0"; font.pixelSize: 22; font.bold: true
            }
            Label {
                Layout.fillWidth: true; wrapMode: Text.Wrap
                text: (root.measured ? qsTranslate("Viewer", "Estimated from the current broadcast.")
                    : qsTranslate("Viewer", "Estimate assumes %1 Mbps until a broadcast is playing.").arg(root.assumedMegabitsPerSecond))
                    + " " + qsTranslate("Viewer", "Actual duration varies with the broadcast and the time limit below.")
                color: "#9ea79f"; font.pixelSize: 13
            }
        }
    }
    RowLayout {
        Layout.fillWidth: true; spacing: 16
        Label { Layout.fillWidth: true; wrapMode: Text.Wrap; text: qsTranslate("Viewer", "Maximum retention (minutes)"); color: "#f4f5f3"; font.pixelSize: 16 }
        ThemedSpinBox {
            id: minutes; objectName: "timeshiftMinutes"
            enabled: enabledToggle.checked
            from: root.saved.min_minutes; to: root.saved.max_minutes; value: root.saved.minutes
            Accessible.name: qsTranslate("Viewer", "Maximum retention (minutes)")
        }
    }
    Label {
        Layout.fillWidth: true; wrapMode: Text.Wrap
        text: qsTranslate("Viewer", "Old data is discarded at either limit. If your paused position expires, playback resumes from the retained range. Memory limits cover retained TS data; decoding uses additional memory.")
        color: "#9ea79f"; font.pixelSize: 13
    }
    RowLayout {
        Layout.fillWidth: true; spacing: 20
        Label {
            Layout.fillWidth: true; wrapMode: Text.Wrap
            text: qsTranslate("Viewer", "Applying changes returns live playback to the live edge and clears its previous history.")
            color: "#9ea79f"; font.pixelSize: 13
        }
        Button {
            id: apply
            objectName: "applyTimeshiftSettings"
            text: qsTranslate("Settings", "Apply")
            implicitWidth: 112; implicitHeight: 44
            contentItem: Label { text: apply.text; color: "#151b16"; font.bold: true; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
            background: Rectangle { radius: 10; color: apply.down ? "#819c86" : apply.hovered ? "#b9cdbd" : "#9caf9f"; border.color: apply.visualFocus ? "#f4f5f3" : "transparent" }
            onClicked: root.backend.configure_timeshift_options(enabledToggle.checked ? root.storageValue : "off", memory.value, files.value, minutes.value)
        }
    }
}
