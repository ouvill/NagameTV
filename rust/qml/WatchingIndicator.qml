import QtQuick
import MinimalViewer

Rectangle {
    width: 6
    height: width
    radius: width / 2
    color: Theme.live
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTranslate("Viewer", "Watching")
    HoverHandler { id: hover }
    ThemedToolTip {
        visible: hover.hovered
        text: qsTranslate("Viewer", "Watching")
    }
}
