import QtQuick
import MinimalViewer

Rectangle {
    required property bool selected
    required property bool hovered
    required property bool pressed
    radius: Theme.panelRadius
    color: pressed ? Theme.surfacePressed : selected ? Theme.surfaceSelected
        : hovered ? Theme.surfaceHover : Theme.surfaceRaised
    border.color: selected || hovered ? Theme.accent : Theme.border
    Behavior on color { ColorAnimation { duration: Theme.colorDuration } }
    Behavior on border.color { ColorAnimation { duration: Theme.colorDuration } }
}
