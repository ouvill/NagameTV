pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Effects

Item {
    id: root
    required property string text
    required property font font
    required property real viewportWidth
    // Position of the un-cropped shadow text in the video, including its offset.
    required property real textX
    readonly property int blurRadius: Math.max(2, Math.ceil(font.pixelSize / 18))
    readonly property real padding: blurRadius * 2
    readonly property real captureWidth: Math.min(Math.ceil(glyph.implicitWidth), viewportWidth)
    readonly property real cropX: Math.max(0, Math.min(-textX, glyph.implicitWidth - captureWidth))
    width: captureWidth
    height: glyph.implicitHeight
    Accessible.ignored: true

    Text {
        id: glyph
        text: root.text
        font: root.font
        textFormat: Text.PlainText
        wrapMode: Text.NoWrap
        renderType: Text.QtRendering
        color: "#80000000"
        Accessible.ignored: true
    }
    // Normal comments keep a static source while their parent moves. Only
    // comments wider than the viewport need a moving crop; texture allocation
    // remains bounded by the visible width, even for very long strings.
    ShaderEffectSource {
        id: texture
        objectName: "commentShadowTexture"
        sourceItem: glyph
        sourceRect: Qt.rect(root.cropX - root.padding, -root.padding,
                            root.captureWidth + 2 * root.padding, glyph.implicitHeight + 2 * root.padding)
        hideSource: true
        visible: false
    }
    MultiEffect {
        objectName: "commentShadowBlur"
        x: root.cropX - root.padding
        y: -root.padding
        width: root.captureWidth + 2 * root.padding
        height: glyph.implicitHeight + 2 * root.padding
        source: texture
        autoPaddingEnabled: false
        blurEnabled: true
        blurMax: root.blurRadius
        blur: 1.0
    }
}
