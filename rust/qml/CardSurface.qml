import QtQuick
import MinimalViewer

// Selection tints the surface; keyboard focus is the shared outline outside it.
// An accent border for either would sit on top of the progress line in the cards.
Rectangle {
    id: surface
    required property bool selected
    required property bool hovered
    required property bool pressed
    property bool focused: false
    radius: Theme.panelRadius
    color: pressed ? Theme.surfacePressed : selected ? Theme.surfaceSelected
        : hovered ? Theme.surfaceHover : Theme.surfaceRaised
    border.color: hovered ? Theme.borderHover : Theme.border
    FocusOutline { focused: surface.focused; cornerRadius: surface.radius }
    Behavior on color { ColorAnimation { duration: Theme.colorDuration } }
    Behavior on border.color { ColorAnimation { duration: Theme.colorDuration } }
}
