pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Shapes

Label {
    id: glyph
    required property var cell
    required property var outlineProvider
    required property string fontFamily
    required property real scaleX
    required property real scaleY
    readonly property real outlineRadius: font.pixelSize * 0.06
    readonly property real captureScaleX: implicitWidth > 0
        ? Math.min(1, cell.glyphWidth * scaleX / implicitWidth) : 1
    text: cell.text
    textFormat: Text.PlainText
    color: cell.foreground
    font.family: fontFamily
    font.pixelSize: Math.max(8, cell.glyphHeight * scaleY)
    font.bold: cell.bold
    font.italic: cell.italic
    font.underline: cell.underline
    renderType: Text.QtRendering
    // Match main's Label layout and baseline. Normalizing to the glyph's ink
    // bounds would move small kana, punctuation, bars and descenders vertically.
    Loader {
        objectName: "outlineLoader"
        x: glyph.leftPadding
        y: glyph.baselineOffset
        width: glyph.width
        height: glyph.height
        z: -1
        active: glyph.cell.stroked
        sourceComponent: Shape {
            preferredRendererType: Shape.CurveRenderer
            ShapePath {
                fillColor: "transparent"
                strokeColor: glyph.cell.stroke
                strokeWidth: glyph.outlineRadius * 2
                joinStyle: ShapePath.RoundJoin
                fillRule: ShapePath.WindingFill
                PathSvg {
                    path: glyph.outlineProvider.subtitle_glyph_outline(glyph.text, glyph.font)
                }
            }
        }
    }
    transform: Scale {
        origin.x: glyph.width / 2
        origin.y: glyph.height / 2
        xScale: glyph.captureScaleX
    }
}
