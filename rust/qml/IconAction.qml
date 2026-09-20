import QtQuick
import QtQuick.Controls

ToolButton {
    id: control
    required property url iconSource
    required property string tip
    property string iconLabel: ""
    property bool active: false
    property bool primary: false
    property bool toolTipEnabled: true
    // Player overlays use bare icons until hover, focus, or selection.
    flat: false
    // Animate the visuals; keep the hit area still while the pointer is down.
    property real feedbackScale: down ? 0.90 : hovered || visualFocus ? 1.06 : 1
    Behavior on feedbackScale {
        NumberAnimation { duration: control.down ? 65 : 150; easing.type: Easing.OutCubic }
    }
    hoverEnabled: true
    opacity: enabled ? 1 : 0.38
    implicitWidth: 42
    implicitHeight: 42
    Accessible.name: tip
    background: Rectangle {
        scale: control.feedbackScale
        radius: 21
        color: control.primary ? (control.down ? "#cbd8ce" : control.hovered ? "#ffffff" : "#eeeeec")
            : control.down ? "#589caf9f" : control.hovered ? "#28ffffff" : control.active ? "#389caf9f" : control.flat ? "transparent" : "#17000000"
        border.color: control.visualFocus ? "#9caf9f" : control.flat ? "transparent" : (control.primary ? "#80ffffff" : (control.active ? "#9caf9f" : "#16ffffff"))
        Behavior on color { ColorAnimation { duration: 100 } }
        Behavior on border.color { ColorAnimation { duration: 100 } }
    }
    contentItem: Item {
        scale: control.feedbackScale
        Image {
            anchors.centerIn: parent
            width: 24
            height: 24
            sourceSize: Qt.size(24, 24)
            source: control.iconSource
        }
        Label {
            anchors.centerIn: parent
            anchors.verticalCenterOffset: -1
            text: control.iconLabel
            visible: text.length > 0
            color: "#f4f5f3"
            font.pixelSize: 8
            font.bold: true
        }
    }
    ToolTip {
        objectName: "actionToolTip"
        parent: control
        visible: control.toolTipEnabled && control.hovered
        text: control.tip
        delay: 0
        timeout: 3000
        x: (control.width - implicitWidth) / 2
        y: -implicitHeight - 10
        padding: 9
        contentItem: Label {
            text: control.tip
            color: "#f4f5f3"
            font.pixelSize: 12
        }
        background: Rectangle {
            radius: 8
            color: "#e61b1d1b"
            border.color: "#38ffffff"
        }
    }
}
