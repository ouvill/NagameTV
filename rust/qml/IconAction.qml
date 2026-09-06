import QtQuick
import QtQuick.Controls

ToolButton {
    id: control
    required property url iconSource
    required property string tip
    property bool active: false
    property bool primary: false
    implicitWidth: 42
    implicitHeight: 42
    Accessible.name: tip
    background: Rectangle {
        radius: 21
        color: control.hovered ? "#28ffffff" : (control.primary ? "#eeeeec" : (control.active ? "#389caf9f" : "#17000000"))
        border.color: control.visualFocus ? "#9caf9f" : (control.primary ? "#80ffffff" : (control.active ? "#9caf9f" : "#16ffffff"))
    }
    contentItem: Item {
        Image {
            anchors.centerIn: parent
            width: 24
            height: 24
            sourceSize: Qt.size(24, 24)
            source: control.iconSource
        }
    }
    ToolTip {
        parent: control
        visible: control.hovered
        text: control.tip
        delay: 150
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
