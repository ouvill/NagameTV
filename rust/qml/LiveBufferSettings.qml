pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    readonly property var saved: JSON.parse(backend.live_buffer_options)
    enum EditState { Synced, Pending, Saving }
    property int editState: LiveBufferSettings.Synced
    readonly property int coalesceMilliseconds: 300
    spacing: 12

    function syncSaved() {
        number.value = saved.milliseconds;
        // Restore invalid/unfinished text without detaching subsequent steps
        // or external setting updates from the SpinBox's value.
        number.contentItem.text = Qt.binding(function() {
            return number.textFromValue(number.value, number.locale);
        });
    }
    function commit() {
        saveDelay.stop();
        editState = LiveBufferSettings.Saving;
        if (number.contentItem.acceptableInput)
            backend.configure_live_buffer(number.valueFromText(number.contentItem.text, number.locale));
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
        spacing: 12
        Label {
            Layout.fillWidth: true
            text: qsTranslate("Settings", "Live playback buffer")
            color: "#f4f5f3"; font.pixelSize: 16
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
        Label { text: "ms"; color: "#9caf9f"; font.pixelSize: 16 }
    }
    Label {
        Layout.fillWidth: true
        text: qsTranslate("Settings", "Increase if audio cuts out. Applies from the next live playback.")
        color: "#b6bab6"; font.pixelSize: 14
        wrapMode: Text.Wrap
    }
    SettingsAction {
        emphasis: SettingsAction.Quiet
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
