import QtQuick
import QtQuick.Controls
import QtQuick.Shapes

ToolButton {
    id: control
    implicitWidth: 42
    implicitHeight: 42
    Accessible.name: qsTranslate("Viewer", "Close channel selection")
    background: Rectangle {
        radius: 21
        color: control.hovered ? "#28ffffff" : "#17000000"
        border.color: control.visualFocus ? "#9caf9f" : "#16ffffff"
    }
    contentItem: Item {
        Shape {
            anchors.centerIn: parent
            width: 24
            height: 24
            ShapePath {
                strokeColor: "#f4f5f3"
                strokeWidth: 2
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                // Same vector geometry as main's Lucide chevron-down icon.
                PathSvg {
                    path: "m 6,9 6,6 6,-6"
                }
            }
        }
    }
    ToolTip.visible: hovered
    ToolTip.text: qsTranslate("Main", "Close")
    ToolTip.delay: 150
}
