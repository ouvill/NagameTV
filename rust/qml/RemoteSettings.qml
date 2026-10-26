pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    readonly property list<Item> navigationItems: webToggle.visible
        ? [enabledToggle, webToggle, addressField, portField, applyButton]
        : [enabledToggle, addressField, portField, applyButton]
    readonly property var webUrls: {
        const urls = JSON.parse(backend.remote_web_urls || "[]");
        return Array.isArray(urls) ? urls : [];
    }
    property bool invalidInput: false
    spacing: Theme.spaceLg
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
        color: Theme.textSecondary
        font.pixelSize: Theme.fontBody
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
    }
    component Field: NavigationField {
        Layout.fillWidth: true
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
    SettingsToggle {
        id: webToggle
        objectName: "remoteWebEnabled"
        Layout.fillWidth: true
        visible: root.backend.remote_web_allowed
        text: qsTranslate("Remote", "Phone remote in the browser (experimental)")
        description: qsTranslate("Remote", "Scan the QR code with a phone on the same network to open a remote control page.")
        checked: root.backend.remote_web_enabled
        onClicked: {
            root.backend.set_remote_web_enabled(checked);
            checked = Qt.binding(function() { return root.backend.remote_web_enabled; });
        }
    }
    Flow {
        objectName: "remoteWebCodes"
        Layout.fillWidth: true
        visible: root.backend.remote_web_enabled && root.webUrls.length > 0
        spacing: Theme.spaceLg
        Repeater {
            model: root.webUrls
            delegate: ColumnLayout {
                id: code
                required property string modelData
                spacing: Theme.spaceSm
                Rectangle {
                    // A light quiet zone keeps the code readable on the dark panel.
                    Layout.preferredWidth: 184
                    Layout.preferredHeight: 184
                    color: Theme.codeBackground
                    radius: Theme.controlRadius
                    Image {
                        anchors.fill: parent
                        anchors.margins: 12
                        source: root.backend.remote_qr_data_uri(code.modelData)
                        sourceSize: Qt.size(160, 160)
                        smooth: false
                        fillMode: Image.PreserveAspectFit
                        Accessible.name: qsTranslate("Remote", "QR code for %1").arg(code.modelData)
                    }
                }
                Label {
                    // The address is what people type when a camera is unavailable, so never elide it.
                    Layout.maximumWidth: 260
                    text: code.modelData
                    textFormat: Text.PlainText
                    color: Theme.textSecondary
                    font.pixelSize: Theme.fontControl
                    wrapMode: Text.WrapAnywhere
                }
            }
        }
    }
    Detail {
        objectName: "remoteWatchers"
        visible: root.backend.remote_status === "listening"
        text: root.backend.remote_watchers === 0 ? qsTranslate("Remote", "No remotes connected.")
            : root.backend.remote_watchers === 1 ? qsTranslate("Remote", "1 remote connected.")
            : qsTranslate("Remote", "%1 remotes connected.").arg(root.backend.remote_watchers)
    }
    Detail {
        objectName: "remoteSessionOnly"
        visible: root.backend.remote_session_only
        text: qsTranslate("Remote", "These settings apply to this launch only and will not be saved.")
    }
    Detail { text: qsTranslate("Remote", "Listen address"); color: Theme.textPrimary }
    Field {
        id: addressField
        objectName: "remoteAddress"
        Accessible.name: qsTranslate("Remote", "Listen address")
    }
    Detail { text: qsTranslate("Remote", "The default 0.0.0.0 accepts connections on all IPv4 interfaces.") }
    Detail { text: qsTranslate("Remote", "Port"); color: Theme.textPrimary }
    Field {
        id: portField
        objectName: "remotePort"
        Accessible.name: qsTranslate("Remote", "Port")
        inputMethodHints: Qt.ImhDigitsOnly
        validator: IntValidator { bottom: 1; top: 65535 }
    }
    ActionButton {
        id: applyButton
        objectName: "applyRemote"
        text: qsTranslate("Remote", "Apply / retry")
        emphasis: ActionButton.Primary
        onClicked: root.apply(root.backend.remote_enabled)
    }
    Detail {
        objectName: "remoteInvalidInput"
        visible: root.invalidInput
        color: Theme.warning
        text: qsTranslate("Remote", "Enter a valid IP address and a port between 1 and 65535.")
    }
    Detail {
        objectName: "remoteStatus"
        color: root.backend.remote_status === "failed" ? Theme.warning : Theme.accent
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
        color: Theme.warning
    }
    Detail {
        objectName: "remoteSaveError"
        visible: root.backend.remote_save_error.length > 0
        text: qsTranslate("Remote", "Could not save the remote settings: %1").arg(root.backend.remote_save_error)
        color: Theme.warning
    }
    Detail {
        visible: root.backend.remote_endpoints.length > 0
        text: qsTranslate("Remote", "Connection addresses")
        color: Theme.textPrimary
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
        color: Theme.textPrimary
        font.pixelSize: Theme.fontControl
        padding: Theme.spaceMd
        background: Rectangle { color: Theme.surfaceRaised; radius: Theme.controlRadius }
    }
}
