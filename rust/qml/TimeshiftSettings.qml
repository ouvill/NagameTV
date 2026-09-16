pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    readonly property var saved: JSON.parse(backend.timeshift_limits)
    spacing: 16
    Label { text: qsTranslate("Viewer", "Timeshift"); color: "#f4f5f3"; font.pixelSize: 24; font.bold: true }
    SettingsToggle {
        id: enabledToggle
        objectName: "timeshiftEnabled"
        Layout.fillWidth: true
        text: qsTranslate("Viewer", "Enable timeshift")
        description: qsTranslate("Viewer", "Pause and rewind live TV.")
        checked: root.backend.timeshift_storage !== "off"
    }
    Label { text: qsTranslate("Viewer", "Storage"); color: "#f4f5f3" }
    ComboBox {
        id: storage
        objectName: "timeshiftStorage"
        enabled: enabledToggle.checked
        model: [qsTranslate("Viewer", "Memory"), qsTranslate("Viewer", "Temporary files")]
        currentIndex: root.backend.timeshift_storage === "filesystem" ? 1 : 0
    }
    Label { text: qsTranslate("Viewer", "Maximum TS memory (MiB)"); color: "#f4f5f3" }
    SpinBox {
        id: memory
        objectName: "timeshiftMemoryLimit"
        editable: true; enabled: enabledToggle.checked
        from: root.saved.min_mib; to: root.saved.max_mib; value: root.saved.memory_mib
    }
    Label { text: qsTranslate("Viewer", "Maximum temporary files (MiB)"); color: "#f4f5f3" }
    SpinBox {
        id: files
        objectName: "timeshiftFileLimit"
        editable: true; enabled: enabledToggle.checked
        from: root.saved.min_mib; to: root.saved.max_mib; value: root.saved.filesystem_mib
    }
    Label { text: qsTranslate("Viewer", "Maximum retention (minutes)"); color: "#f4f5f3" }
    SpinBox {
        id: minutes
        objectName: "timeshiftMinutes"
        editable: true; enabled: enabledToggle.checked
        from: root.saved.min_minutes; to: root.saved.max_minutes; value: root.saved.minutes
    }
    Label {
        Layout.fillWidth: true
        wrapMode: Text.Wrap
        text: qsTranslate("Viewer", "Old data is discarded at either limit. If your paused position expires, playback resumes from the retained range. Memory limits cover retained TS data; decoding uses additional memory.")
        color: "#b6bab6"
    }
    Label {
        Layout.fillWidth: true; wrapMode: Text.Wrap
        text: qsTranslate("Viewer", "Applying changes returns live playback to the live edge and clears its previous history.")
        color: "#b6bab6"
    }
    Button {
        objectName: "applyTimeshiftSettings"
        text: qsTranslate("Settings", "Apply")
        onClicked: root.backend.configure_timeshift_options(
            enabledToggle.checked ? (storage.currentIndex === 0 ? "memory" : "filesystem") : "off",
            memory.value, files.value, minutes.value)
    }
}
