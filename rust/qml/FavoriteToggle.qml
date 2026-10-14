import QtQuick
import MinimalViewer

// The star is a secondary action inside a card that already owns the click and
// the keyboard focus; it takes pointer presses only and never joins the tab order.
IconAction {
    id: control
    required property bool favorite
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    signal favoriteToggled
    iconSource: iconDirectory + (favorite ? "star-filled.svg" : "star.svg")
    tip: favorite ? qsTranslate("Viewer", "Remove from favorites") : qsTranslate("Viewer", "Add to favorites")
    flat: true
    iconSize: Theme.smallIconSize
    implicitWidth: Theme.compactControlHeight
    implicitHeight: Theme.compactControlHeight
    focusPolicy: Qt.NoFocus
    onClicked: favoriteToggled()
}
