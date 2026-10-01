import QtQuick
import MinimalViewer

// A visual only: the owning control keeps its layout and input geometry.
Rectangle {
    objectName: "focusOutline"
    required property bool focused
    property real cornerRadius: Theme.controlRadius
    anchors.fill: parent
    anchors.margins: -Theme.focusOutset
    radius: cornerRadius + Theme.focusOutset
    color: "transparent"
    border.width: Theme.focusWidth
    border.color: Theme.accent
    border.pixelAligned: false
    antialiasing: true
    opacity: enabled && focused ? 1 : 0
    Behavior on opacity { NumberAnimation { duration: Theme.focusDuration } }
}
