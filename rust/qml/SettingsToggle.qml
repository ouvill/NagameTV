pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

ToggleSwitch {
    id: control
    focusTarget: ToggleSwitch.Row
    property string description: ""
    implicitHeight: Math.max(56, contentItem.implicitHeight + 24)
    leftPadding: Theme.spaceMd
    rightPadding: Theme.spaceMd
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
                font.pixelSize: Theme.fontBody
                wrapMode: Text.Wrap
            }
        }
        Item {
            Layout.preferredWidth: control.indicator.implicitWidth
            Layout.preferredHeight: control.indicator.implicitHeight
        }
    }
    background: Item {
        Rectangle {
            anchors.fill: parent
            anchors.margins: Theme.focusOutset
            color: control.down ? Theme.selection : control.hovered || control.focusVisible ? Theme.overlayHover : "transparent"
            radius: Theme.controlRadius
            Behavior on color { ColorAnimation { duration: Theme.colorDuration } }
            FocusOutline { focused: control.focusVisible }
        }
        Rectangle {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: 1
            color: Theme.surfaceRaised
        }
    }
}
