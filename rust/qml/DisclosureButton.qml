import QtQuick
import MinimalViewer

ActionButton {
    id: control
    checkable: true
    selected: checked
    textAlignment: Text.AlignLeft
    rightPadding: Theme.spaceLg + 24
    Image {
        anchors { right: parent.right; rightMargin: Theme.spaceLg; verticalCenter: parent.verticalCenter }
        width: 16; height: 16
        sourceSize: Qt.size(16, 16)
        source: "qrc:/qt/qml/MinimalViewer/assets/icons/chevron-down.svg"
        rotation: control.checked ? 0 : -90
        Behavior on rotation { NumberAnimation { duration: Theme.moveDuration } }
    }
}
