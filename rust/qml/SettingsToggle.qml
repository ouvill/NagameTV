pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ToggleSwitch {
    id: control
    property string description: ""
    implicitHeight: Math.max(72, contentItem.implicitHeight + 28)
    leftPadding: 0
    rightPadding: 4
    topPadding: 14
    bottomPadding: 14
    Accessible.description: description
    contentItem: RowLayout {
        spacing: 24
        ColumnLayout {
            Layout.fillWidth: true
            spacing: 6
            Label {
                Layout.fillWidth: true
                text: control.text
                textFormat: Text.PlainText
                color: "#f4f5f3"
                font.pixelSize: 18
                wrapMode: Text.Wrap
            }
            Label {
                Layout.fillWidth: true
                visible: text.length > 0
                text: control.description
                textFormat: Text.PlainText
                color: "#b6bab6"
                font.pixelSize: 13
                wrapMode: Text.Wrap
            }
        }
        Item {
            Layout.preferredWidth: control.indicator.implicitWidth
            Layout.preferredHeight: control.indicator.implicitHeight
        }
    }
    background: Rectangle {
        color: control.down ? "#189caf9f" : control.hovered ? "#0affffff" : "transparent"
        radius: 8
        border.color: control.visualFocus ? "#9caf9f" : "transparent"
        Behavior on color { ColorAnimation { duration: 100 } }
        Rectangle {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: 1
            color: "#1c1f1c"
        }
    }
}
