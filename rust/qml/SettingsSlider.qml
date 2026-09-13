pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root
    required property string text
    required property string valueText
    required property real from
    required property real to
    required property real stepSize
    required property real value
    signal moved(real value)
    implicitHeight: Math.max(72, row.implicitHeight + 28)
    opacity: enabled ? 1 : 0.42
    RowLayout {
        id: row
        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
        spacing: 16
        Label {
            Layout.fillWidth: true
            text: root.text
            color: "#f4f5f3"
            font.pixelSize: 18
            wrapMode: Text.Wrap
        }
        ThemedSlider {
            objectName: "settingSlider"
            Layout.preferredWidth: Math.min(320, root.width * 0.42)
            from: root.from; to: root.to; stepSize: root.stepSize
            value: root.value
            Accessible.name: root.text
            onMoved: root.moved(value)
        }
        Label {
            Layout.preferredWidth: 72
            text: root.valueText
            color: "#9caf9f"
            font.pixelSize: 16
            horizontalAlignment: Text.AlignRight
        }
    }
    Rectangle {
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: 1
        color: "#1c1f1c"
    }
}
