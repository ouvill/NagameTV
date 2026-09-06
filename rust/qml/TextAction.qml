import QtQuick
import QtQuick.Controls

Button {
    id: textAction
    padding: 12
    contentItem: Label {
        text: textAction.text
        color: "#f4f5f3"
        font.pixelSize: 12
        horizontalAlignment: Text.AlignHCenter
    }
    background: Rectangle {
        radius: 16
        color: textAction.down || textAction.hovered ? "#28ffffff" : "#1c1f1c"
        border.color: textAction.visualFocus ? "#9caf9f" : "#28ffffff"
    }
}
