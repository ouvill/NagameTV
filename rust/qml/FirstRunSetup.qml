pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: root
    required property var backend
    property Window targetWindow: null
    signal completed
    signal openFileRequested
    signal fileDropped(url file)
    signal transferDropped(string key)
    parent: Overlay.overlay
    x: 0; y: 0
    width: parent.width; height: parent.height
    padding: 0
    modal: true
    dim: false
    focus: true
    closePolicy: Popup.NoAutoClose
    background: Rectangle { color: Theme.surface }
    onAboutToShow: form.reset()
    onOpened: form.focusInput()
    onClosed: form.phase = ConnectionForm.Idle
    contentItem: Item {
        RecordingDropArea {
            anchors.fill: parent
            onFileDropped: function(file) { root.fileDropped(file); }
            onTransferDropped: function(key) { root.transferDropped(key); }
        }
        Loader {
            anchors { left: parent.left; right: parent.right; top: parent.top }
            height: 76
            active: root.targetWindow !== null
            sourceComponent: WindowDragArea { targetWindow: root.targetWindow }
        }
        Loader {
            anchors { right: parent.right; top: parent.top; margins: 18 }
            active: root.targetWindow !== null
            sourceComponent: WindowButtons { targetWindow: root.targetWindow }
        }
        ScrollView {
            id: scroll
            anchors.fill: parent
            anchors.topMargin: 88
            anchors.bottomMargin: 32
            anchors.leftMargin: Math.max(32, (root.width - 600) / 2)
            anchors.rightMargin: anchors.leftMargin
            contentWidth: availableWidth
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ColumnLayout {
                width: scroll.availableWidth
                spacing: Theme.spaceXl
                Label {
                    Layout.fillWidth: true
                    text: qsTranslate("Connection", "Connect to Mirakurun")
                    color: Theme.textPrimary
                    font.pixelSize: Theme.fontDisplay; font.bold: true
                    wrapMode: Text.Wrap
                }
                Label {
                    Layout.fillWidth: true
                    text: qsTranslate("Connection", "To watch TV, you need a configured Mirakurun server. Enter its URL to get started.")
                    color: Theme.textSecondary
                    font.pixelSize: Theme.fontControl
                    wrapMode: Text.Wrap
                }
                Label {
                    Layout.fillWidth: true
                    visible: root.backend.settings_error.length > 0
                    text: qsTranslate("Settings", "Could not read or save settings. Your changes may not be available the next time you open the app.")
                    color: Theme.warning
                    font.pixelSize: Theme.fontBody
                    wrapMode: Text.Wrap
                }
                ConnectionForm {
                    id: form
                    objectName: "setupConnectionForm"
                    Layout.fillWidth: true
                    backend: root.backend
                    onCompleted: {
                        root.close();
                        root.completed();
                    }
                }
                ActionButton {
                    objectName: "setupOpenRecording"
                    Layout.alignment: Qt.AlignHCenter
                    text: qsTranslate("Recording", "Open TS file")
                    onClicked: root.openFileRequested()
                }
            }
        }
    }
}
