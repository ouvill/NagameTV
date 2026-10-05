pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import MinimalViewer

Item {
    id: overlay
    required property string captionJson
    required property var outlineProvider
    property bool forceOutline: false
    // Plain-text cues have no broadcast plane coordinates. Scale their
    // fallback layout from the same 720-line picture used for comments.
    readonly property real plainTextScale: height / 720
    property url fontSource: "qrc:/qt/qml/MinimalViewer/assets/fonts/rounded-mplus-1m-arib.ttf"
    readonly property var cue: captionJson.length ? JSON.parse(captionJson) : null
    readonly property var cells: cue ? cue.cells : []
    clip: true
    ScreenshotText { id: captureText }
    function screenshotLayer(target) {
        const origin = mapToItem(target, 0, 0);
        const commands = [];
        for (let i = 0; i < captionCells.count; ++i) {
            const cell = captionCells.itemAt(i) as CaptionCell;
            if (!cell || !cell.visible) continue;
            commands.push({kind: "rect", x: cell.x, y: cell.y, width: cell.width, height: cell.height, color: cell.color.toString()});
            if (cell.drcs) {
                commands.push({kind: "drcs", index: cell.index, force_outline: overlay.forceOutline,
                    x: cell.x + cell.bitmap.x, y: cell.y + cell.bitmap.y,
                    width: cell.bitmap.width, height: cell.bitmap.height});
                continue;
            }
            const glyph = cell.captureGlyph;
            commands.push(captureText.command(glyph, cell.x + (cell.width - glyph.width) / 2, cell.y + (cell.height - glyph.height) / 2,
                glyph.captureScaleX, glyph.outlineColor.toString(), glyph.outlined ? glyph.outlineRadius * 2 : 0,
                null, false, false));
        }
        if (plainCaption.visible && plainCaption.text.length)
            commands.push(captureText.command(plainCaption, plainCaption.x, plainCaption.y, 1,
                plainCaption.styleColor.toString(), 2, null, true, true));
        return {x: origin.x, y: origin.y, width: width, height: height, commands: commands};
    }
    FontLoader {
        id: subtitleFont
        objectName: "subtitleFont"
        source: overlay.fontSource
        onStatusChanged: if (status === FontLoader.Error) console.warn("Subtitle font could not be loaded:", source)
    }
    component CaptionCell: Rectangle {
        id: captionCell
        readonly property var captureGlyph: textLoader.item
        required property var modelData
        required property int index
        readonly property bool drcs: modelData.glyph !== undefined && modelData.glyph.kind === "drcs"
        readonly property alias bitmap: bitmapLoader
        readonly property int drcsPadding: Math.max(1, Math.ceil(modelData.glyphHeight * 0.06)) + (modelData.bold ? 1 : 0)
        readonly property real sx: overlay.width / Math.max(1, overlay.cue.planeWidth)
        readonly property real sy: overlay.height / Math.max(1, overlay.cue.planeHeight)
        x: modelData.x * sx; y: modelData.y * sy
        width: Math.max(1, modelData.width * sx)
        height: Math.max(1, modelData.height * sy)
        color: modelData.background
        Loader {
            id: bitmapLoader
            active: captionCell.drcs
            anchors.centerIn: parent
            width: (captionCell.modelData.glyphWidth + 2 * captionCell.drcsPadding) * captionCell.sx
            height: (captionCell.modelData.glyphHeight + 2 * captionCell.drcsPadding) * captionCell.sy
            sourceComponent: MediaCaption {
                objectName: "drcsGlyph"
                // Presentation revision changes even when only DRCS pixels change.
                image: { const revision = overlay.cue.revision; return overlay.outlineProvider.subtitle_drcs_image(captionCell.index, overlay.forceOutline); }
                stretch: true
            }
        }
        Loader {
            id: textLoader
            active: !captionCell.drcs
            anchors.centerIn: parent
            sourceComponent: SubtitleGlyph {
                objectName: "subtitleGlyph"
                forceOutline: overlay.forceOutline
                cell: captionCell.modelData
                outlineProvider: overlay.outlineProvider
                scaleX: captionCell.sx
                scaleY: captionCell.sy
                fontFamily: subtitleFont.status === FontLoader.Ready ? subtitleFont.name : "Noto Sans CJK JP"
            }
        }
    }
    Repeater {
        id: captionCells
        objectName: "captionCells"
        model: overlay.cells
        delegate: CaptionCell {}
    }
    Label {
        id: plainCaption
        objectName: "plainCaption"
        visible: overlay.cells.length === 0
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 38 * overlay.plainTextScale
        width: Math.min(parent.width * 0.82, 1040 * overlay.plainTextScale)
        text: overlay.cue ? overlay.cue.text : ""
        textFormat: Text.PlainText
        color: "white"
        font.pixelSize: Math.max(1, Math.round(28 * overlay.plainTextScale))
        font.bold: true
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        style: Text.Outline
        styleColor: "#e0000000"
    }
}
