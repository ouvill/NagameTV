pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import MinimalViewer

ToolTip {
    id: root
    required property real positionMs
    required property string imageSource
    property bool available: true
    signal requested(real milliseconds)
    signal dismissed()
    readonly property real imageWidth: 240
    readonly property real imageHeight: 135
    readonly property int refreshIntervalMs: 120
    property bool imageSpaceReserved: false
    readonly property bool currentImage: visible && available
        && imageSource.length > 0
    readonly property bool showImageSpace: available && imageSpaceReserved
    onCurrentImageChanged: if (currentImage) imageSpaceReserved = true
    padding: Theme.spaceSm
    font.pixelSize: Theme.fontCaption

    function refresh() {
        if (available) {
            root.requested(positionMs);
        } else {
            root.dismissed();
        }
    }
    onVisibleChanged: {
        if (visible) refresh();
        else { imageSpaceReserved = false; root.dismissed(); }
    }
    onAvailableChanged: if (visible) refresh()
    Component.onDestruction: root.dismissed()
    Timer {
        interval: root.refreshIntervalMs
        running: root.visible
        repeat: true
        onTriggered: root.refresh()
    }
    contentItem: Column {
        spacing: Theme.spaceXs
        Image {
            objectName: "seekPreviewImage"
            width: root.showImageSpace ? root.imageWidth : 0
            height: root.showImageSpace ? root.imageHeight : 0
            visible: root.showImageSpace
            source: root.currentImage ? root.imageSource : ""
            fillMode: Image.PreserveAspectFit
            cache: false
        }
        Label {
            width: Math.max(root.showImageSpace ? root.imageWidth : 0, Math.min(implicitWidth, root.imageWidth))
            text: root.text
            textFormat: Text.PlainText
            font: root.font
            color: Theme.textPrimary
            wrapMode: Text.Wrap
            horizontalAlignment: Text.AlignHCenter
        }
    }
    background: Rectangle {
        radius: Theme.controlRadius
        color: Theme.overlaySurface
        border.color: Theme.overlayBorder
    }
}
