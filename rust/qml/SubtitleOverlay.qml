pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Item {
    id: overlay
    required property string captionJson
    required property var outlineProvider
    property bool forceOutline: false
    property url fontSource: "qrc:/qt/qml/MinimalViewer/assets/fonts/rounded-mplus-1m-arib.ttf"
    readonly property var cue: captionJson.length ? JSON.parse(captionJson) : null
    readonly property var cells: cue ? cue.cells : []
    clip: true
    ScreenshotText { id: captureText }
    function screenshotLayer(target) {
        const origin = mapToItem(target, 0, 0);
        const commands = [];
        for (let i = 0; i < captionCells.count; ++i) {
            const cell = captionCells.itemAt(i);
            if (!cell || !cell.visible) continue;
            commands.push({kind: "rect", x: cell.x, y: cell.y, width: cell.width, height: cell.height, color: cell.color.toString()});
            const glyph = cell.captureGlyph;
            commands.push(captureText.command(glyph, cell.x + glyph.x, cell.y + glyph.y,
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
    Repeater {
        id: captionCells
        objectName: "captionCells"
        model: overlay.cells
        delegate: Rectangle {
            id: cell
            readonly property alias captureGlyph: renderedGlyph
            required property var modelData
            readonly property real sx: overlay.width / Math.max(1, overlay.cue.planeWidth)
            readonly property real sy: overlay.height / Math.max(1, overlay.cue.planeHeight)
            x: modelData.x * sx; y: modelData.y * sy
            width: Math.max(1, modelData.width * sx)
            height: Math.max(1, modelData.height * sy)
            color: modelData.background
            SubtitleGlyph {
                id: renderedGlyph
                objectName: "subtitleGlyph"
                anchors.centerIn: parent
                forceOutline: overlay.forceOutline
                cell: parent.modelData
                outlineProvider: overlay.outlineProvider
                scaleX: parent.sx
                scaleY: parent.sy
                fontFamily: subtitleFont.status === FontLoader.Ready ? subtitleFont.name : "Noto Sans CJK JP"
            }
        }
    }
    Label {
        id: plainCaption
        objectName: "plainCaption"
        visible: overlay.cells.length === 0
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 38
        width: Math.min(parent.width * 0.82, 1040)
        text: overlay.cue ? overlay.cue.text : ""
        textFormat: Text.PlainText
        color: "white"
        font.pixelSize: 28
        font.bold: true
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.Wrap
        style: Text.Outline
        styleColor: "#e0000000"
    }
}
