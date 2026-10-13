import QtQuick
import MinimalViewer

// A recording's still, or a quiet placeholder while there is none.
Rectangle {
    id: root
    required property string source
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    implicitWidth: 128
    implicitHeight: 72
    radius: Theme.indicatorRadius
    color: Theme.surface
    clip: true
    Image {
        id: picture
        anchors.fill: parent
        source: root.source
        // Decode at the displayed size; the server's still is much larger.
        sourceSize: Qt.size(Math.max(1, root.width * 2), Math.max(1, root.height * 2))
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: false
    }
    Image {
        anchors.centerIn: parent
        visible: picture.status !== Image.Ready
        width: Theme.iconSize
        height: Theme.iconSize
        sourceSize: Qt.size(Theme.iconSize, Theme.iconSize)
        source: root.iconDirectory + "recording.svg"
        opacity: Theme.disabledOpacity
    }
}
