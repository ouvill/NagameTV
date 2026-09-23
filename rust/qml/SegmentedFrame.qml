import QtQuick
import MinimalViewer

Rectangle {
    readonly property real outerCornerRadius: Theme.panelRadius
    readonly property real segmentCornerRadius: Theme.controlRadius
    radius: outerCornerRadius
    color: Theme.overlaySurface
    border.color: Theme.overlayBorder
}
