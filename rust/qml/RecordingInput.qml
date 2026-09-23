pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Dialogs as FileDialogs

Item {
    id: root
    required property var backend
    signal started

    function open() { picker.open(); }
    function openTransfer(key) {
        if (backend.open_recording_transfer(key)) return true;
        errorDialog.open();
        return false;
    }
    function openUrl(url) {
        if (backend.open_recording(url)) {
            return true;
        }
        errorDialog.open();
        return false;
    }
    Connections {
        target: root.backend
        function onRecordingOpened(success) {
            if (success) root.started();
            else errorDialog.open();
        }
    }
    Popup {
        objectName: "recordingOpening"
        parent: Overlay.overlay
        anchors.centerIn: parent
        visible: root.backend.recording_loading
        padding: Theme.spaceXl
        background: PanelSurface {}
        closePolicy: Popup.NoAutoClose
        contentItem: Row {
            spacing: Theme.spaceMd
            BusyIndicator { running: root.backend.recording_loading }
            Label {
                anchors.verticalCenter: parent.verticalCenter
                text: qsTranslate("Recording", "Opening TS file…")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontBody
            }
            ActionButton {
                objectName: "cancelRecordingOpen"
                anchors.verticalCenter: parent.verticalCenter
                text: qsTranslate("Recording", "Cancel")
                onClicked: root.backend.cancel_recording_open()
            }
        }
    }
    RecordingDropArea {
        anchors.fill: parent
        onFileDropped: function(file) { root.openUrl(file); }
        onTransferDropped: function(key) { root.openTransfer(key); }
    }
    FileDialogs.FileDialog {
        id: picker
        objectName: "recordingPicker"
        title: qsTranslate("Recording", "Open TS file")
        fileMode: FileDialogs.FileDialog.OpenFile
        nameFilters: [qsTranslate("Recording", "Transport streams (*.ts *.TS *.m2ts *.M2TS)"), qsTranslate("Recording", "All files (*)")]
        onAccepted: root.openUrl(selectedFile)
    }
    Dialog {
        id: errorDialog
        objectName: "recordingOpenError"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(480, parent.width - 32)
        modal: true
        title: qsTranslate("Recording", "Could not open recording")
        standardButtons: Dialog.Ok
        padding: Theme.spaceXl
        background: PanelSurface {}
        header: Label {
            text: errorDialog.title
            padding: Theme.spaceXl
            color: Theme.textPrimary
            font.pixelSize: Theme.fontHeading
            wrapMode: Text.Wrap
        }
        footer: DialogButtonBox {
            standardButtons: errorDialog.standardButtons
            delegate: ActionButton {}
            background: Item {}
            padding: Theme.spaceLg
            onAccepted: errorDialog.accept()
        }
        contentItem: Label {
            color: Theme.textPrimary
            font.pixelSize: Theme.fontBody
            text: root.backend.file_error.length ? root.backend.file_error : root.backend.playback_error
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
    }
}
