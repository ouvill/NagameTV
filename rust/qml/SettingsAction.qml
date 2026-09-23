pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Button {
    id: control
    enum Emphasis { Secondary, Primary, Quiet }
    property int emphasis: SettingsAction.Secondary
    property real feedbackScale: down ? 0.97 : 1
    implicitWidth: contentItem.implicitWidth + leftPadding + rightPadding
    implicitHeight: 40
    leftPadding: 16
    rightPadding: 16
    hoverEnabled: true
    opacity: enabled ? 1 : 0.42
    Behavior on feedbackScale {
        NumberAnimation { duration: control.down ? 65 : 150; easing.type: Easing.OutCubic }
    }
    contentItem: Label {
        scale: control.feedbackScale
        text: control.text
        color: control.emphasis === SettingsAction.Primary ? "#17201a"
            : control.emphasis === SettingsAction.Quiet ? "#b6bab6" : "#f4f5f3"
        font.pixelSize: 14
        font.bold: control.emphasis === SettingsAction.Primary
        verticalAlignment: Text.AlignVCenter
        horizontalAlignment: Text.AlignHCenter
    }
    background: Rectangle {
        scale: control.feedbackScale
        radius: 8
        color: control.emphasis === SettingsAction.Primary
            ? (control.down ? "#8da793" : control.hovered ? "#b6c6b8" : "#9caf9f")
            : control.down ? "#344238" : control.hovered ? "#2a302b"
            : control.emphasis === SettingsAction.Quiet ? "transparent" : "#222622"
        border.color: control.visualFocus ? "#9caf9f" : "transparent"
        Behavior on color { ColorAnimation { duration: 100 } }
    }
}
