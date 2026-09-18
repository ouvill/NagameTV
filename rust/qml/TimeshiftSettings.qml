pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    readonly property var saved: JSON.parse(backend.timeshift_limits)
    enum EditState { Synced, Pending, Saving }
    property int editState: TimeshiftSettings.Synced
    property string storageValue: "memory"
    property string saveError: ""
    readonly property int coalesceMilliseconds: 300
    function syncSaved() {
        if (backend.timeshift_storage !== "off") storageValue = backend.timeshift_storage;
        enabledToggle.checked = backend.timeshift_storage !== "off";
        memory.value = saved.memory_mib;
        files.value = saved.filesystem_mib;
        minutes.value = saved.minutes;
    }
    function commit() {
        saveDelay.stop();
        readNumericEdits();
        editState = TimeshiftSettings.Saving;
        const accepted = backend.configure_timeshift_options(enabledToggle.checked ? storageValue : "off", memory.value, files.value, minutes.value);
        saveError = accepted ? "" : qsTranslate("Viewer", "Could not change timeshift settings. Previous settings remain in use.");
        syncSaved();
        editState = TimeshiftSettings.Synced;
    }
    function scheduleSave() {
        editState = TimeshiftSettings.Pending;
        saveDelay.restart();
    }
    function readNumericEdits() {
        for (const field of [memory, files, minutes]) {
            if (!field.contentItem.acceptableInput) continue;
            const value = field.valueFromText(field.contentItem.text, field.locale);
            if (value !== field.value) {
                field.value = value;
                editState = TimeshiftSettings.Pending;
            }
        }
    }
    function finishNumericEdit() {
        readNumericEdits();
        if (editState === TimeshiftSettings.Pending) commit();
    }
    component BudgetSpinBox: ThemedSpinBox {
        id: spin
        onValueModified: root.scheduleSave()
        Connections {
            target: spin.contentItem
            function onTextEdited() {
                root.editState = TimeshiftSettings.Pending;
                saveDelay.stop();
            }
            function onEditingFinished() { root.finishNumericEdit(); }
        }
    }
    Component.onCompleted: syncSaved()
    Component.onDestruction: {
        finishNumericEdit();
    }
    Timer {
        id: saveDelay
        interval: root.coalesceMilliseconds
        onTriggered: root.commit()
    }
    Connections {
        target: root.backend
        function onTimeshift_storageChanged() {
            if (root.editState === TimeshiftSettings.Synced) root.syncSaved();
        }
        function onTimeshift_limitsChanged() {
            if (root.editState === TimeshiftSettings.Synced) root.syncSaved();
        }
    }
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
        onClicked: root.commit()
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
            onSelected: function(value) { root.storageValue = value; root.commit(); }
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
                BudgetSpinBox {
                    id: memory; objectName: "timeshiftMemoryLimit"
                    visible: root.storageValue === "memory"; enabled: enabledToggle.checked
                    from: root.saved.min_mib; to: root.saved.max_mib
                    Accessible.name: qsTranslate("Viewer", "Maximum TS memory (MiB)")
                }
                BudgetSpinBox {
                    id: files; objectName: "timeshiftFileLimit"
                    visible: root.storageValue === "filesystem"; enabled: enabledToggle.checked
                    from: root.saved.min_mib; to: root.saved.max_mib
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
        BudgetSpinBox {
            id: minutes; objectName: "timeshiftMinutes"
            enabled: enabledToggle.checked
            from: root.saved.min_minutes; to: root.saved.max_minutes
            Accessible.name: qsTranslate("Viewer", "Maximum retention (minutes)")
        }
    }
    Label {
        Layout.fillWidth: true; wrapMode: Text.Wrap
        text: qsTranslate("Viewer", "Old data is discarded at either limit. If your paused position expires, playback resumes from the retained range. Memory limits cover retained TS data; decoding uses additional memory.")
        color: "#9ea79f"; font.pixelSize: 13
    }
    Label {
        Layout.fillWidth: true; wrapMode: Text.Wrap
        text: qsTranslate("Viewer", "Changes are saved automatically. Reducing limits moves playback only if its position is no longer retained. Changing storage or turning timeshift off clears history and returns to live playback.")
        color: "#9ea79f"; font.pixelSize: 13
    }
    Label {
        objectName: "timeshiftSaveError"
        Layout.fillWidth: true; wrapMode: Text.Wrap
        visible: text.length > 0
        text: root.saveError
        color: "#ffb080"; font.pixelSize: 13
    }
}
