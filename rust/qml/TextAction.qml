import QtQuick
import QtQuick.Controls

Button {
    id: textAction
    property real feedbackScale: down ? 0.96 : 1
    Behavior on feedbackScale {
        NumberAnimation { duration: textAction.down ? 65 : 150; easing.type: Easing.OutCubic }
    }
    hoverEnabled: true
    padding: 12
    contentItem: Label {
        scale: textAction.feedbackScale
        text: textAction.text
        color: "#f4f5f3"
        font.pixelSize: 12
        horizontalAlignment: Text.AlignHCenter
    }
    background: Rectangle {
        scale: textAction.feedbackScale
        radius: 16
        color: textAction.down ? "#429caf9f" : textAction.hovered ? "#28ffffff" : "#1c1f1c"
        border.color: textAction.visualFocus ? "#9caf9f" : "#28ffffff"
        Behavior on color { ColorAnimation { duration: 100 } }
    }
}
