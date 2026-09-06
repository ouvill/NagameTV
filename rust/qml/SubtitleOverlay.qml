import QtQuick

Item {
    required property string captionJson
    readonly property var cue: captionJson.length ? JSON.parse(captionJson) : null
    clip: true
    Repeater {
        model: parent.cue ? parent.cue.cells : []
        delegate: Rectangle {
            id: cell
            required property var modelData
            readonly property real sx: parent.width / Math.max(1, parent.cue.planeWidth)
            readonly property real sy: parent.height / Math.max(1, parent.cue.planeHeight)
            x: modelData.x * sx; y: modelData.y * sy
            width: Math.max(1, modelData.width * sx)
            height: Math.max(1, modelData.height * sy)
            color: modelData.background
            Text {
                id: glyph
                anchors.centerIn: parent
                text: cell.modelData.text
                textFormat: Text.PlainText
                color: cell.modelData.foreground
                font.family: "Noto Sans CJK JP"
                font.pixelSize: Math.max(8, cell.modelData.glyphHeight * cell.sy)
                font.bold: cell.modelData.bold
                font.italic: cell.modelData.italic
                font.underline: cell.modelData.underline
                renderType: Text.NativeRendering
                style: cell.modelData.stroked ? Text.Outline : Text.Normal
                styleColor: cell.modelData.stroke
                transform: Scale {
                    origin.x: glyph.width / 2; origin.y: glyph.height / 2
                    xScale: glyph.implicitWidth > 0
                        ? Math.min(1, cell.modelData.glyphWidth * cell.sx / glyph.implicitWidth) : 1
                }
            }
        }
    }
}
