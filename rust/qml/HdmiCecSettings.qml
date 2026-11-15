pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    readonly property list<Item> navigationItems: deviceChoice.visible ? [enabledToggle, deviceChoice, retryButton] : [enabledToggle]
    // The device model is a list model; the choice needs "Automatic" first.
    property var devices: []
    readonly property var choices: [{ path: "", label: qsTranslate("HdmiCec", "Automatic") }].concat(devices.map(device => ({
        path: device.path,
        label: device.name.length > 0 ? "%1 (%2)".arg(device.name).arg(device.path) : device.path
    })))
    spacing: Theme.spaceLg

    component Detail: Label {
        Layout.fillWidth: true
        color: Theme.textSecondary
        font.pixelSize: Theme.fontBody
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
    }
    component DeviceRow: QtObject {
        required property string path
        required property string name
    }
    Instantiator {
        id: deviceRows
        model: root.backend.hdmi_cec_devices
        delegate: DeviceRow {}
        onObjectAdded: root.collectDevices()
        onObjectRemoved: root.collectDevices()
    }
    function collectDevices() {
        const rows = [];
        for (let i = 0; i < deviceRows.count; ++i) {
            const row = deviceRows.objectAt(i) as DeviceRow;
            if (row) rows.push({ path: row.path, name: row.name });
        }
        devices = rows;
    }

    SettingsToggle {
        id: enabledToggle
        objectName: "hdmiCecEnabled"
        Layout.fillWidth: true
        enabled: root.backend.hdmi_cec_allowed
        text: qsTranslate("HdmiCec", "Control with the TV remote over HDMI-CEC (experimental)")
        description: qsTranslate("HdmiCec", "Use the remote of a TV connected by HDMI to operate this viewer. The TV and the HDMI port must support CEC.")
        checked: root.backend.hdmi_cec_enabled
        onClicked: {
            root.backend.set_hdmi_cec_enabled(checked);
            checked = Qt.binding(function() { return root.backend.hdmi_cec_enabled; });
        }
    }
    Detail { visible: deviceChoice.visible; text: qsTranslate("HdmiCec", "CEC device"); color: Theme.textPrimary }
    SettingsChoice {
        id: deviceChoice
        objectName: "hdmiCecDevice"
        visible: root.backend.hdmi_cec_enabled
        Layout.fillWidth: true
        Accessible.name: qsTranslate("HdmiCec", "CEC device")
        model: root.choices.map(choice => choice.label)
        currentIndex: Math.max(0, root.choices.findIndex(choice => choice.path === root.backend.hdmi_cec_device))
        onActivated: function(index) {
            root.backend.select_hdmi_cec_device(root.choices[index].path);
            currentIndex = Qt.binding(function() {
                return Math.max(0, root.choices.findIndex(choice => choice.path === root.backend.hdmi_cec_device));
            });
        }
    }
    ActionButton {
        id: retryButton
        objectName: "retryHdmiCec"
        visible: deviceChoice.visible
        text: qsTranslate("HdmiCec", "Search again")
        onClicked: root.backend.refresh_hdmi_cec_devices()
    }
    Detail {
        objectName: "hdmiCecStatus"
        color: root.backend.hdmi_cec_state === "active" ? Theme.accent
            : root.backend.hdmi_cec_state === "disabled" || root.backend.hdmi_cec_state === "waiting-for-tv" ? Theme.textSecondary
            : Theme.warning
        text: {
            switch (root.backend.hdmi_cec_state) {
            case "disabled": return qsTranslate("HdmiCec", "HDMI-CEC is off.");
            case "no-device": return qsTranslate("HdmiCec", "No CEC device found. Connect a device that supports CEC, then search again.");
            case "permission-denied": return qsTranslate("HdmiCec", "Cannot open the CEC device. Allow this user to access it (for example, add it to the video group) and sign in again.");
            case "waiting-for-tv": return qsTranslate("HdmiCec", "Waiting for the TV. Turn on the TV and check the HDMI connection.");
            case "active": return root.backend.hdmi_cec_physical_address.length > 0
                ? qsTranslate("HdmiCec", "Ready. The TV sees this viewer as “%1” at HDMI %2.").arg(root.backend.hdmi_cec_osd_name).arg(root.backend.hdmi_cec_physical_address)
                : qsTranslate("HdmiCec", "Ready. The TV sees this viewer as “%1”.").arg(root.backend.hdmi_cec_osd_name);
            case "disconnected": return qsTranslate("HdmiCec", "The CEC device was disconnected. Searching again…");
            case "error": return qsTranslate("HdmiCec", "HDMI-CEC stopped because of an error.");
            case "unsupported": return qsTranslate("HdmiCec", "HDMI-CEC is not available on this system.");
            }
            return "";
        }
    }
    Detail {
        objectName: "hdmiCecError"
        visible: text.length > 0
        text: root.backend.hdmi_cec_error
        color: Theme.warning
    }
    Detail {
        visible: root.backend.hdmi_cec_enabled
        text: qsTranslate("HdmiCec", "Which buttons reach the viewer depends on the TV maker’s CEC implementation.")
    }
}
