pragma ComponentBehavior: Bound
import QtQuick

QtObject {
    function command(label, x, y, scaleX, stroke, strokeWidth, shadow, wrap, center) {
        return {
            kind: "text", x: x, y: y, width: label.width,
            baseline: label.baselineOffset, scale_x: scaleX,
            text: label.text, family: label.font.family, size: label.font.pixelSize,
            bold: label.font.bold, italic: label.font.italic, underline: label.font.underline,
            color: label.color.toString(), stroke: stroke, stroke_width: strokeWidth,
            opacity: label.opacity, shadow: shadow, wrap: wrap, center: center
        };
    }
}
