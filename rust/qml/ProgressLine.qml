import QtQuick
import MinimalViewer

// How far a program or a recording has advanced. A visual only.
Rectangle {
    id: root
    required property real progress
    property color fillColor: Theme.accent
    // Flush against a picture's edge instead of floating as a capsule.
    property bool square: false
    implicitHeight: Theme.progressHeight
    radius: square ? 0 : height / 2
    color: Theme.progressTrack
    Rectangle {
        width: Math.max(0, Math.min(1, root.progress)) * parent.width
        height: parent.height
        radius: parent.radius
        color: root.fillColor
    }
}
