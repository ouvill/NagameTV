import QtQuick
import QtQuick.Controls
import MinimalViewer

ToolTip {
    id: tip
    padding: Theme.spaceSm
    font.pixelSize: Theme.fontCaption
    contentItem: Label {
        text: tip.text
        textFormat: Text.PlainText
        font: tip.font
        color: Theme.textPrimary
        wrapMode: Text.Wrap
    }
    background: Rectangle {
        radius: Theme.controlRadius
        color: Theme.overlaySurface
        border.color: Theme.overlayBorder
    }
}
