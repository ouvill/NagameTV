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
    onVisibleChanged: {
        if (visible && backend.hdmi_cec_enabled)
            backend.refresh_hdmi_cec_devices();
    }

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
        color: {
            switch (root.backend.hdmi_cec_state) {
            case "configured": return Theme.accent;
            case "external": return root.backend.hdmi_cec_external_without_passthrough ? Theme.warning : Theme.textSecondary;
            case "disabled": return Theme.textSecondary;
            default: return Theme.warning;
            }
        }
        text: {
            switch (root.backend.hdmi_cec_state) {
            case "disabled": return qsTranslate("HdmiCec", "HDMI-CEC is off.");
            case "no-device": return qsTranslate("HdmiCec", "No CEC device found. Connect a device that supports CEC, then search again.");
            case "permission-denied": return qsTranslate("HdmiCec", "Cannot access the CEC device. Check access permissions, then search again.");
            case "no-remote-control": return qsTranslate("HdmiCec", "This CEC device cannot forward remote-control keys to Linux.");
            case "configured": return root.backend.hdmi_cec_physical_address.length > 0
                ? qsTranslate("HdmiCec", "Configured as “%1” at HDMI %2.").arg(root.backend.hdmi_cec_osd_name).arg(root.backend.hdmi_cec_physical_address)
                : qsTranslate("HdmiCec", "Configured as “%1”. The HDMI address is not available.").arg(root.backend.hdmi_cec_osd_name);
            case "external": return root.backend.hdmi_cec_external_without_passthrough
                ? qsTranslate("HdmiCec", "Using an existing CEC configuration. It does not enable key forwarding, so remote buttons may not reach the viewer.")
                : qsTranslate("HdmiCec", "Using an existing CEC configuration.");
            case "error": return qsTranslate("HdmiCec", "Could not configure or release the CEC device.");
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
        text: qsTranslate("HdmiCec", "The viewer must have focus to receive remote buttons. Available buttons depend on the TV.")
    }
}
