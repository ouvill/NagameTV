pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

ToggleSwitch {
    id: control
    property string description: ""
    implicitHeight: Math.max(56, contentItem.implicitHeight + 24)
    leftPadding: 0
    rightPadding: Theme.spaceXs
    topPadding: Theme.spaceMd
    bottomPadding: Theme.spaceMd
    Accessible.description: description
    contentItem: RowLayout {
        spacing: Theme.spaceXl
        ColumnLayout {
            Layout.fillWidth: true
            spacing: Theme.spaceXs
            Label {
                Layout.fillWidth: true
                text: control.text
                textFormat: Text.PlainText
                color: Theme.textPrimary
                font.pixelSize: Theme.fontControl
                wrapMode: Text.Wrap
            }
            Label {
                Layout.fillWidth: true
                visible: text.length > 0
                text: control.description
                textFormat: Text.PlainText
                color: Theme.textSecondary
                font.pixelSize: Theme.fontCaption
                wrapMode: Text.Wrap
            }
        }
        Item {
            Layout.preferredWidth: control.indicator.implicitWidth
            Layout.preferredHeight: control.indicator.implicitHeight
        }
    }
    background: Rectangle {
        color: control.down ? Theme.selection : control.hovered ? Theme.overlayHover : "transparent"
        radius: Theme.controlRadius
        border.color: control.visualFocus ? Theme.accent : "transparent"
        Behavior on color { ColorAnimation { duration: Theme.colorDuration } }
        Rectangle {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: 1
            color: Theme.surfaceRaised
        }
    }
}
