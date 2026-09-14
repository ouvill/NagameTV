pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    property bool invalidInput: false
    spacing: 16
    function reset() {
        addressField.text = backend.remote_address;
        portField.text = String(backend.remote_port);
        invalidInput = false;
        backend.refresh_remote_addresses();
    }
    function apply(enabled, useSavedAddress = false) {
        // Turning off must work even while the address fields contain an invalid draft.
        const valid = (useSavedAddress || portField.acceptableInput) && backend.configure_remote(enabled,
            useSavedAddress ? backend.remote_address : addressField.text,
            useSavedAddress ? backend.remote_port : Number(portField.text));
        invalidInput = !valid;
        if (valid) reset();
        enabledToggle.checked = Qt.binding(function() { return root.backend.remote_enabled; });
    }
    onVisibleChanged: if (visible) reset()
    Component.onCompleted: reset()

    component Detail: Label {
        Layout.fillWidth: true
        color: "#b6bab6"
        font.pixelSize: 14
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
    }
    component Field: TextField {
        id: field
        Layout.fillWidth: true
        implicitHeight: 48
        color: "#f4f5f3"
        font.pixelSize: 16
        selectByMouse: true
        leftPadding: 14; rightPadding: 14
        selectionColor: "#527359"
        background: Rectangle {
            radius: 8
            color: "#1c1f1c"
            border.color: field.activeFocus ? "#9caf9f" : "#8c918c"
        }
        onTextEdited: root.invalidInput = false
        onAccepted: root.apply(root.backend.remote_enabled)
    }
    SettingsToggle {
        id: enabledToggle
        objectName: "remoteEnabled"
        Layout.fillWidth: true
        text: qsTranslate("Remote", "Enable remote control")
        description: qsTranslate("Remote", "Allow devices on your home network to control this viewer without authentication.")
        checked: root.backend.remote_enabled
        onClicked: root.apply(checked, !checked)
    }
    Detail {
        objectName: "remoteSessionOnly"
        visible: root.backend.remote_session_only
        text: qsTranslate("Remote", "These settings apply to this launch only and will not be saved.")
    }
    Detail { text: qsTranslate("Remote", "Listen address"); color: "#f4f5f3" }
    Field {
        id: addressField
        objectName: "remoteAddress"
        Accessible.name: qsTranslate("Remote", "Listen address")
    }
    Detail { text: qsTranslate("Remote", "The default 0.0.0.0 accepts connections on all IPv4 interfaces.") }
    Detail { text: qsTranslate("Remote", "Port"); color: "#f4f5f3" }
    Field {
        id: portField
        objectName: "remotePort"
        Accessible.name: qsTranslate("Remote", "Port")
        inputMethodHints: Qt.ImhDigitsOnly
        validator: IntValidator { bottom: 1; top: 65535 }
    }
    Detail { text: qsTranslate("Remote", "The default port is 50051. Use a different port for each additional viewer.") }
    Button {
        id: applyButton
        objectName: "applyRemote"
        text: qsTranslate("Remote", "Apply / retry")
        implicitHeight: 48
        leftPadding: 24; rightPadding: 24
        contentItem: Label { text: applyButton.text; color: "#0b0c0b"; font.pixelSize: 16; font.bold: true; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
        background: Rectangle { radius: 8; color: applyButton.down ? "#8da793" : "#9caf9f"; border.color: applyButton.visualFocus ? "#f4f5f3" : "transparent" }
        onClicked: root.apply(root.backend.remote_enabled)
    }
    Detail {
        objectName: "remoteInvalidInput"
        visible: root.invalidInput
        color: "#ffb080"
        text: qsTranslate("Remote", "Enter a valid IP address and a port between 1 and 65535.")
    }
    Detail {
        objectName: "remoteStatus"
        color: root.backend.remote_status === "failed" ? "#ffb080" : "#9caf9f"
        text: {
            switch (root.backend.remote_status) {
            case "disabled": return qsTranslate("Remote", "Remote control is off.");
            case "starting": return qsTranslate("Remote", "Starting remote control…");
            case "listening": return qsTranslate("Remote", "Remote control is ready.");
            case "stopping": return qsTranslate("Remote", "Stopping the previous listener…");
            case "failed": return qsTranslate("Remote", "Could not start remote control. Check the address and port, then retry.");
            }
            return "";
        }
    }
    Detail {
        objectName: "remoteError"
        visible: text.length > 0
        text: root.backend.remote_error
        color: "#ffb080"
    }
    Detail {
        objectName: "remoteSaveError"
        visible: root.backend.remote_save_error.length > 0
        text: qsTranslate("Remote", "Could not save the remote settings: %1").arg(root.backend.remote_save_error)
        color: "#ffb080"
    }
    Detail {
        visible: root.backend.remote_endpoints.length > 0
        text: qsTranslate("Remote", "Connection addresses")
        color: "#f4f5f3"
    }
    TextArea {
        objectName: "remoteEndpoints"
        Layout.fillWidth: true
        visible: root.backend.remote_endpoints.length > 0
        text: root.backend.remote_endpoints
        textFormat: TextEdit.PlainText
        readOnly: true
        selectByMouse: true
        wrapMode: TextEdit.WrapAnywhere
        color: "#f4f5f3"
        font.pixelSize: 16
        padding: 12
        background: Rectangle { color: "#1c1f1c"; radius: 8 }
    }
}
