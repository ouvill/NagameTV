import QtQuick
import QtQuick.Controls

Rectangle {
    width: 6
    height: width
    radius: width / 2
    color: "#e36b6b"
    Accessible.role: Accessible.StaticText
    Accessible.name: qsTranslate("Viewer", "Watching")
    HoverHandler { id: hover }
    ToolTip.visible: hover.hovered
    ToolTip.text: qsTranslate("Viewer", "Watching")
}
