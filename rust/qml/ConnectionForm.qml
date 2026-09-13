pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    enum Phase { Idle, Checking, Failed, Empty, Ready, SaveFailed }
    property int phase: ConnectionForm.Idle
    property int channelCount: 0
    property string errorDetails: ""
    property bool detailsVisible: false
    readonly property bool busy: phase === ConnectionForm.Checking || backend.loading
    signal completed
    spacing: 16

    function reset() {
        phase = ConnectionForm.Idle;
        server.text = backend.server;
        errorDetails = "";
        detailsVisible = false;
    }
    function focusInput() { server.forceActiveFocus(); }
    function connectToServer() {
        if (busy || !server.text.trim().length) return;
        phase = ConnectionForm.Checking;
        detailsVisible = false;
        errorDetails = "";
        if (!backend.connect_server(server.text)) {
            errorDetails = backend.status;
            phase = ConnectionForm.Failed;
        }
    }
    Connections {
        target: root.backend
        function onConnectionFinished(success, channels) {
            if (root.phase !== ConnectionForm.Checking) return;
            root.channelCount = channels;
            if (!success) {
                root.errorDetails = root.backend.status;
                root.phase = ConnectionForm.Failed;
            } else if (root.backend.settings_error.length) {
                root.errorDetails = root.backend.settings_error;
                root.phase = ConnectionForm.SaveFailed;
            } else {
                root.phase = channels > 0 ? ConnectionForm.Ready : ConnectionForm.Empty;
            }
            if (root.visible) action.forceActiveFocus();
        }
    }

    component Detail: Label {
        Layout.fillWidth: true
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        color: "#b6bab6"
        font.pixelSize: 14
    }
    Detail {
        text: qsTranslate("Settings", "Server URL")
        color: "#f4f5f3"
        font.pixelSize: 16
        font.bold: true
    }
    TextField {
        id: server
        objectName: "serverField"
        Layout.fillWidth: true
        implicitHeight: 58
        text: root.backend.server
        readOnly: root.busy
        placeholderText: "http://192.168.1.100:40772"
        color: "#f4f5f3"
        placeholderTextColor: "#8c918c"
        font.pixelSize: 18
        leftPadding: 16; rightPadding: 16
        Accessible.name: qsTranslate("Settings", "Server URL")
        background: Rectangle {
            radius: 10; color: "#2b2926"
            border.color: server.activeFocus ? "#9caf9f" : "#8c918c"
        }
        onAccepted: root.connectToServer()
        onTextEdited: {
            root.phase = ConnectionForm.Idle;
            root.errorDetails = "";
            root.detailsVisible = false;
        }
    }
    Detail { text: qsTranslate("Settings", "Enter the Mirakurun server address starting with http:// or https://.") }
    BusyIndicator {
        Layout.alignment: Qt.AlignHCenter
        implicitWidth: 40; implicitHeight: 40
        visible: root.phase === ConnectionForm.Checking
        running: visible
        palette.dark: "#9caf9f"
    }
    Detail {
        objectName: "connectionResult"
        visible: root.phase !== ConnectionForm.Idle
        color: root.phase === ConnectionForm.Failed || root.phase === ConnectionForm.SaveFailed ? "#ffb080" : "#b6c6b8"
        text: {
            switch (root.phase) {
            case ConnectionForm.Checking:
                return qsTranslate("Connection", "Checking the connection…");
            case ConnectionForm.Failed:
                return qsTranslate("Connection", "Could not connect. Check the server URL and network connection, then try again.");
            case ConnectionForm.Empty:
                return qsTranslate("Connection", "Connected, but no channels were found. Check the tuner and channel settings on your Mirakurun server, then try again.");
            case ConnectionForm.Ready:
                return qsTranslate("Connection", "Connected. %1 channels were found.").arg(root.channelCount);
            case ConnectionForm.SaveFailed:
                return qsTranslate("Connection", "Connected, but the settings could not be saved. Check the details and try again.");
            default: return "";
            }
        }
    }
    Detail {
        visible: root.phase === ConnectionForm.Ready
        text: qsTranslate("Connection", "Choose a channel to start watching. You can change subtitles and comments later in Settings.")
    }
    TextAction {
        objectName: "connectionDetailsToggle"
        visible: root.errorDetails.length > 0
        text: root.detailsVisible ? qsTranslate("Settings", "Hide details") : qsTranslate("Settings", "Show details")
        onClicked: root.detailsVisible = !root.detailsVisible
    }
    Detail {
        objectName: "connectionErrorDetails"
        visible: root.detailsVisible
        text: root.errorDetails
    }
    Button {
        id: action
        objectName: "connectServer"
        Layout.fillWidth: true
        implicitHeight: 54
        enabled: !root.busy && server.text.trim().length > 0
        text: root.phase === ConnectionForm.Ready ? qsTranslate("Viewer", "Choose a channel")
            : root.phase === ConnectionForm.Checking ? qsTranslate("Connection", "Checking the connection…")
            : root.phase === ConnectionForm.Idle ? qsTranslate("Connection", "Connect") : qsTranslate("Main", "Retry")
        hoverEnabled: true
        opacity: enabled ? 1 : 0.5
        onClicked: {
            if (root.phase === ConnectionForm.Ready) root.completed();
            else root.connectToServer();
        }
        contentItem: Label {
            text: action.text
            color: "#0b0c0b"
            font.pixelSize: 16; font.bold: true
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: 10
            color: action.down ? "#8da793" : action.hovered ? "#b6c6b8" : "#9caf9f"
            border.color: action.visualFocus ? "#f4f5f3" : "#8c918c"
        }
    }
    TextAction {
        objectName: "serverSetupHelp"
        Layout.alignment: Qt.AlignHCenter
        text: qsTranslate("Connection", "Need to set up a Mirakurun server?")
        onClicked: Qt.openUrlExternally("https://github.com/Chinachu/Mirakurun/blob/master/doc/Platforms.md")
    }
}
