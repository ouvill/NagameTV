pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MinimalViewer

Dialog {
    id: root
    objectName: "recordingLibrary"
    required property Player backend
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(960, parent.width - Theme.spaceLg * 2)
    height: Math.min(720, parent.height - Theme.spaceLg * 2)
    modal: true
    padding: Theme.spaceLg
    background: PanelSurface {}
    title: qsTranslate("RecordingLibrary", "EPGStation recordings")
    function search() { backend.browse_epgstation(serverField.text, keywordField.text); }
    onOpened: {
        if (!serverField.text.length) serverField.text = backend.epgstation_server;
        if (serverField.text.length && !backend.epgstation_loaded && !backend.epgstation_busy) search();
        if (serverField.text.length) keywordField.forceActiveFocus();
        else serverField.forceActiveFocus();
    }
    onClosed: { backend.cancel_epgstation(); loginDialog.close(); }
    Connections {
        target: root.backend
        function onRecordingOpened(success: bool) { if (success) root.close(); }
    }
    header: Label {
        text: root.title
        padding: Theme.spaceLg
        color: Theme.textPrimary
        font.pixelSize: Theme.fontHeading
    }
    contentItem: ColumnLayout {
        spacing: Theme.spaceMd
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceMd
            SettingsField {
                id: serverField
                objectName: "epgstationServer"
                Layout.fillWidth: true
                placeholderText: "http://epgstation:8888"
                Accessible.name: qsTranslate("RecordingLibrary", "EPGStation URL")
                inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText
                onAccepted: root.search()
            }
            ActionButton {
                objectName: "epgstationConnect"
                text: qsTranslate("RecordingLibrary", "Connect")
                enabled: serverField.text.trim().length > 0 && !root.backend.epgstation_busy
                onClicked: root.search()
            }
            ActionButton {
                objectName: "epgstationLogin"
                text: qsTranslate("RecordingLibrary", "Sign in…")
                enabled: serverField.text.trim().length > 0 && !root.backend.epgstation_busy
                onClicked: loginDialog.open()
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceMd
            SettingsField {
                id: keywordField
                objectName: "epgstationKeyword"
                Layout.fillWidth: true
                maximumLength: 256
                placeholderText: qsTranslate("RecordingLibrary", "Search recordings")
                Accessible.name: placeholderText
                onAccepted: root.search()
            }
            ActionButton {
                objectName: "epgstationSearch"
                text: qsTranslate("RecordingLibrary", "Search")
                enabled: serverField.text.trim().length > 0 && !root.backend.epgstation_busy
                onClicked: root.search()
            }
        }
        Label {
            Layout.fillWidth: true
            visible: text.length > 0
            text: root.backend.epgstation_error
            color: Theme.error
            font.pixelSize: Theme.fontCaption
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
        Label {
            Layout.fillWidth: true
            visible: text.length > 0
            text: root.backend.settings_error
            color: Theme.error
            font.pixelSize: Theme.fontCaption
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true
            ListView {
                id: list
                objectName: "epgstationRecordings"
                anchors.fill: parent
                clip: true
                spacing: Theme.spaceMd
                model: root.backend.recordings
                ScrollBar.vertical: ScrollBar {}
                delegate: Item {
                    id: row
                    required property string recordedId
                    required property string programName
                    required property string channelName
                    required property real startMs
                    required property real endMs
                    required property string description
                    required property bool playable
                    required property string unavailableReason
                    width: list.width
                    height: content.implicitHeight + Theme.spaceLg * 2
                    CardSurface { anchors.fill: parent; selected: false; hovered: false; pressed: false }
                    RowLayout {
                        id: content
                        anchors { left: parent.left; right: parent.right; top: parent.top; margins: Theme.spaceLg }
                        spacing: Theme.spaceLg
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceXs
                            Label {
                                Layout.fillWidth: true
                                text: row.programName
                                color: Theme.textPrimary
                                font.pixelSize: Theme.fontBody
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                            }
                            Label {
                                Layout.fillWidth: true
                                text: row.channelName + "  " + new Date(row.startMs).toLocaleString(Qt.locale(root.backend.ui_language), "yyyy/MM/dd HH:mm")
                                      + " – " + new Date(row.endMs).toLocaleTimeString(Qt.locale(root.backend.ui_language), "HH:mm")
                                color: Theme.textSecondary
                                textFormat: Text.PlainText
                                font.pixelSize: Theme.fontCaption
                                elide: Text.ElideRight
                            }
                            Label {
                                Layout.fillWidth: true
                                visible: text.length > 0
                                text: row.playable ? row.description : row.unavailableReason === "recording"
                                      ? qsTranslate("RecordingLibrary", "Recording in progress")
                                      : qsTranslate("RecordingLibrary", "No recorded TS file")
                                color: Theme.textSecondary
                                font.pixelSize: Theme.fontCaption
                                textFormat: Text.PlainText
                                elide: Text.ElideRight
                            }
                        }
                        ActionButton {
                            objectName: "epgstationPlay"
                            text: qsTranslate("RecordingLibrary", "Play")
                            enabled: row.playable && !root.backend.recording_loading
                            onClicked: root.backend.play_epgstation(row.recordedId)
                        }
                    }
                }
            }
            Label {
                anchors.centerIn: parent
                width: parent.width
                visible: !root.backend.epgstation_busy && list.count === 0
                horizontalAlignment: Text.AlignHCenter
                wrapMode: Text.Wrap
                text: root.backend.epgstation_loaded ? qsTranslate("RecordingLibrary", "No recordings found")
                      : qsTranslate("RecordingLibrary", "Connect to EPGStation to browse recordings.")
                color: Theme.textSecondary
                font.pixelSize: Theme.fontBody
            }
            BusyIndicator {
                anchors.centerIn: parent
                running: root.backend.epgstation_busy
                visible: running
            }
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceMd
            ActionButton {
                text: qsTranslate("RecordingLibrary", "Previous")
                enabled: root.backend.epgstation_previous
                onClicked: root.backend.epgstation_change_page(false)
            }
            Label {
                Layout.fillWidth: true
                text: qsTranslate("RecordingLibrary", "Page %1 · %2 recordings").arg(root.backend.epgstation_page).arg(root.backend.epgstation_total)
                horizontalAlignment: Text.AlignHCenter
                color: Theme.textSecondary
                font.pixelSize: Theme.fontCaption
            }
            ActionButton {
                text: qsTranslate("RecordingLibrary", "Next")
                enabled: root.backend.epgstation_next
                onClicked: root.backend.epgstation_change_page(true)
            }
            ActionButton {
                objectName: "epgstationClose"
                text: qsTranslate("RecordingLibrary", "Close")
                onClicked: root.close()
            }
        }
    }
    Dialog {
        id: loginDialog
        objectName: "epgstationLoginDialog"
        parent: Overlay.overlay
        anchors.centerIn: parent
        width: Math.min(480, parent.width - Theme.spaceLg * 2)
        modal: true
        padding: Theme.spaceLg
        background: PanelSurface {}
        onOpened: username.forceActiveFocus()
        onClosed: password.clear()
        function signIn() {
            if (!username.text.length || !password.text.length) return;
            if (root.backend.login_epgstation(serverField.text, username.text, password.text)) {
                keywordField.clear();
                close();
            }
        }
        contentItem: ColumnLayout {
            spacing: Theme.spaceMd
            Label {
                text: qsTranslate("RecordingLibrary", "Sign in to EPGStation (stuayu)")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontHeading
            }
            SettingsField {
                id: username
                objectName: "epgstationUsername"
                Layout.fillWidth: true
                placeholderText: qsTranslate("RecordingLibrary", "Username")
                Accessible.name: placeholderText
                onAccepted: password.forceActiveFocus()
            }
            SettingsField {
                id: password
                objectName: "epgstationPassword"
                Layout.fillWidth: true
                placeholderText: qsTranslate("RecordingLibrary", "Password")
                Accessible.name: placeholderText
                echoMode: TextInput.Password
                onAccepted: loginDialog.signIn()
            }
            Label {
                Layout.fillWidth: true
                text: qsTranslate("RecordingLibrary", "Your password is not saved. Sign in again after restarting the app.")
                color: Theme.textSecondary
                font.pixelSize: Theme.fontCaption
                wrapMode: Text.Wrap
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: Theme.spaceMd
                ActionButton {
                    text: qsTranslate("RecordingLibrary", "Cancel")
                    onClicked: loginDialog.close()
                }
                ActionButton {
                    objectName: "epgstationSubmitLogin"
                    text: qsTranslate("RecordingLibrary", "Sign in")
                    emphasis: ActionButton.Primary
                    enabled: username.text.length > 0 && password.text.length > 0
                    onClicked: loginDialog.signIn()
                }
            }
        }
    }
}
