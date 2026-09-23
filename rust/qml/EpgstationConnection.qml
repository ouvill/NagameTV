pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MinimalViewer

Item {
    id: root
    required property string serverUrl
    required property bool busy
    required property bool connected
    required property string error
    signal connectRequested(string server)
    signal loginRequested(string server, string username, string password)
    signal cancelRequested
    signal recordingsRequested
    implicitWidth: content.implicitWidth
    implicitHeight: content.implicitHeight

    function reset() { serverField.text = serverUrl; loginDialog.close(); }
    function focusInput() { serverField.forceActiveFocus(); }
    function closeLogin() { loginDialog.close(); }
    function connectToServer() {
        if (!busy && serverField.text.trim().length) connectRequested(serverField.text.trim());
    }
    onVisibleChanged: if (!visible) closeLogin()

    ColumnLayout {
        id: content
        width: parent.width
        spacing: Theme.spaceMd
        Label {
            text: qsTranslate("RecordingLibrary", "EPGStation URL")
            color: Theme.textPrimary
            font.pixelSize: Theme.fontControl
            font.bold: true
        }
        SettingsField {
            id: serverField
            objectName: "epgstationServer"
            Layout.fillWidth: true
            text: root.serverUrl
            readOnly: root.busy
            placeholderText: "http://epgstation:8888"
            Accessible.name: qsTranslate("RecordingLibrary", "EPGStation URL")
            inputMethodHints: Qt.ImhUrlCharactersOnly | Qt.ImhNoPredictiveText
            onAccepted: root.connectToServer()
        }
        RowLayout {
            spacing: Theme.spaceMd
            ActionButton {
                objectName: "epgstationConnect"
                text: qsTranslate("RecordingLibrary", "Connect")
                emphasis: ActionButton.Primary
                enabled: serverField.text.trim().length > 0 && !root.busy
                onClicked: root.connectToServer()
            }
            ActionButton {
                objectName: "epgstationLogin"
                text: qsTranslate("RecordingLibrary", "Sign in…")
                enabled: serverField.text.trim().length > 0 && !root.busy
                onClicked: loginDialog.open()
            }
            ActionButton {
                objectName: "epgstationCancel"
                visible: root.busy
                text: qsTranslate("RecordingLibrary", "Cancel")
                onClicked: root.cancelRequested()
            }
            BusyIndicator {
                visible: root.busy
                running: visible
                implicitWidth: Theme.controlHeight
                implicitHeight: Theme.controlHeight
            }
        }
        Label {
            objectName: "epgstationConnectionError"
            Layout.fillWidth: true
            visible: text.length > 0
            text: root.error
            textFormat: Text.PlainText
            color: Theme.error
            font.pixelSize: Theme.fontBody
            wrapMode: Text.Wrap
        }
        Label {
            objectName: "epgstationConnected"
            Layout.fillWidth: true
            visible: root.connected && !root.busy && !root.error.length
            text: qsTranslate("RecordingLibrary", "Connected to %1").arg(root.serverUrl)
            textFormat: Text.PlainText
            color: Theme.accentHover
            font.pixelSize: Theme.fontBody
            wrapMode: Text.Wrap
        }
        ActionButton {
            objectName: "epgstationBrowse"
            visible: root.connected && !root.busy
            text: qsTranslate("RecordingLibrary", "Open recordings")
            onClicked: root.recordingsRequested()
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
            if (root.busy || !username.text.length || !password.text.length) return;
            root.loginRequested(serverField.text.trim(), username.text, password.text);
            close();
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
                    enabled: !root.busy && username.text.length > 0 && password.text.length > 0
                    onClicked: loginDialog.signIn()
                }
            }
        }
    }
}
