pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs as FileDialogs

Item {
    id: root
    required property var backend
    signal started
    signal libraryRequested

    function open() { sourceDialog.open(); }
    function openFile() { sourceDialog.close(); picker.open(); }
    function submitUrl() {
        if (urlField.text.trim().length && openUrl(urlField.text.trim())) sourceDialog.close();
    }
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
    Dialog {
        id: sourceDialog
        objectName: "recordingSource"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(560, parent.width - Theme.spaceXl * 2)
        modal: true
        title: qsTranslate("Recording", "Open recording")
        padding: Theme.spaceXl
        background: PanelSurface {}
        onOpened: urlField.forceActiveFocus()
        header: Label {
            text: sourceDialog.title
            padding: Theme.spaceXl
            color: Theme.textPrimary
            font.pixelSize: Theme.fontHeading
        }
        contentItem: ColumnLayout {
            spacing: Theme.spaceLg
            RowLayout {
                Layout.fillWidth: true
                spacing: Theme.spaceMd
                ActionButton {
                    objectName: "recordingChooseFile"
                    text: qsTranslate("Recording", "Open video file")
                    Layout.fillWidth: true
                    onClicked: root.openFile()
                }
                ActionButton {
                    objectName: "recordingBrowseEpgstation"
                    text: qsTranslate("RecordingLibrary", "EPGStation recordings")
                    Layout.fillWidth: true
                    onClicked: { sourceDialog.close(); root.libraryRequested(); }
                }
            }
            Label {
                text: qsTranslate("Recording", "Recording URL")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontBody
            }
            SettingsField {
                id: urlField
                objectName: "recordingUrl"
                Layout.fillWidth: true
                placeholderText: "http://epgstation:8888/api/videos/123"
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText
                Accessible.name: qsTranslate("Recording", "Recording URL")
                onAccepted: root.submitUrl()
            }
            Label {
                Layout.fillWidth: true
                text: qsTranslate("Recording", "Paste the direct URL of a recorded video file.")
                color: Theme.textSecondary
                font.pixelSize: Theme.fontCaption
                wrapMode: Text.Wrap
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: Theme.spaceMd
                ActionButton {
                    text: qsTranslate("Recording", "Cancel")
                    onClicked: sourceDialog.reject()
                }
                ActionButton {
                    objectName: "recordingOpenUrl"
                    text: qsTranslate("Recording", "Open URL")
                    emphasis: ActionButton.Primary
                    enabled: urlField.text.trim().length > 0
                    onClicked: root.submitUrl()
                }
            }
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
                text: qsTranslate("Recording", "Opening video file…")
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
        title: qsTranslate("Recording", "Open video file")
        fileMode: FileDialogs.FileDialog.OpenFile
        nameFilters: [qsTranslate("Recording", "Video files (*.ts *.TS *.m2ts *.M2TS *.mp4 *.MP4 *.mkv *.MKV *.webm *.WEBM *.mov *.MOV)"), qsTranslate("Recording", "All files (*)")]
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
