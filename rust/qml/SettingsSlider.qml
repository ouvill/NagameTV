pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
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
    implicitHeight: Math.max(56, row.implicitHeight + 24)
    RowLayout {
        id: row
        anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
        spacing: Theme.spaceLg
        Label {
            Layout.fillWidth: true
            text: root.text
            opacity: root.enabled ? 1 : Theme.disabledOpacity
            color: Theme.textPrimary
            font.pixelSize: Theme.fontControl
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
            opacity: root.enabled ? 1 : Theme.disabledOpacity
            color: Theme.accent
            font.pixelSize: Theme.fontBody
            horizontalAlignment: Text.AlignRight
        }
    }
    Rectangle {
        anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
        height: 1
        color: Theme.surfaceRaised
    }
}
