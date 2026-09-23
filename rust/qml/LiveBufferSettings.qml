pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    readonly property var saved: JSON.parse(backend.live_buffer_options)
    enum EditState { Synced, Pending, Saving }
    property int editState: LiveBufferSettings.Synced
    readonly property int coalesceMilliseconds: 300
    spacing: Theme.spaceMd

    function syncSaved() {
        number.value = saved.milliseconds;
        // Restore invalid/unfinished text without detaching subsequent steps
        // or external setting updates from the SpinBox's value.
        number.restoreInput();
    }
    function commit() {
        saveDelay.stop();
        editState = LiveBufferSettings.Saving;
        if (number.commitInput())
            backend.configure_live_buffer(number.value);
        syncSaved();
        editState = LiveBufferSettings.Synced;
    }
    function finishEdit() {
        if (editState === LiveBufferSettings.Pending) commit();
    }
    onVisibleChanged: if (!visible) finishEdit()
    Component.onDestruction: finishEdit()
    Component.onCompleted: syncSaved()
    Connections {
        target: root.backend
        function onLive_buffer_optionsChanged() {
            if (root.editState === LiveBufferSettings.Synced) root.syncSaved();
        }
    }
    Timer {
        id: saveDelay
        interval: root.coalesceMilliseconds
        onTriggered: root.commit()
    }
    RowLayout {
        Layout.fillWidth: true
        spacing: Theme.spaceMd
        Label {
            Layout.fillWidth: true
            text: qsTranslate("Settings", "Live playback buffer")
            color: Theme.textPrimary; font.pixelSize: Theme.fontControl
            wrapMode: Text.Wrap
        }
        ThemedSpinBox {
            id: number
            objectName: "liveBufferMilliseconds"
            from: root.saved.min_ms
            to: root.saved.max_ms
            Accessible.name: qsTranslate("Settings", "Live playback buffer (milliseconds)")
            onValueModified: {
                root.editState = LiveBufferSettings.Pending;
                saveDelay.restart();
            }
            Connections {
                target: number.contentItem
                function onTextEdited() {
                    root.editState = LiveBufferSettings.Pending;
                    saveDelay.stop();
                }
                function onEditingFinished() { root.finishEdit(); }
            }
        }
        Label { text: "ms"; color: Theme.accent; font.pixelSize: Theme.fontControl }
    }
    Label {
        Layout.fillWidth: true
        text: qsTranslate("Settings", "Increase if audio cuts out. Applies from the next live playback.")
        color: Theme.textSecondary; font.pixelSize: Theme.fontBody
        wrapMode: Text.Wrap
    }
    ActionButton {
        emphasis: ActionButton.Quiet
        objectName: "resetLiveBuffer"
        text: qsTranslate("Settings", "Reset to %1 ms").arg(root.saved.default_ms)
        onClicked: {
            saveDelay.stop();
            root.editState = LiveBufferSettings.Saving;
            root.backend.configure_live_buffer(root.saved.default_ms);
            root.syncSaved();
            root.editState = LiveBufferSettings.Synced;
        }
    }
}
