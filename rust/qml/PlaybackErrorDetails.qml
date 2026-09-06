import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: root
    required property string details
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(640, parent.width - 40)
    height: Math.min(420, parent.height - 80)
    modal: true
    focus: true
    padding: 20
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: Rectangle {
        radius: 18
        color: "#151715"
        border.color: "#42ffffff"
    }
    contentItem: ColumnLayout {
        spacing: 12
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: "エラー詳細"
                color: "#f4f5f3"
                font.pixelSize: 18
                Layout.fillWidth: true
            }
            TextAction {
                text: "閉じる"
                onClicked: root.close()
            }
        }
        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            contentWidth: availableWidth
            TextArea {
                objectName: "playbackErrorText"
                text: root.details
                textFormat: TextEdit.PlainText
                readOnly: true
                selectByMouse: true
                wrapMode: TextEdit.Wrap
                color: "#b6bab6"
                font.pixelSize: 12
                background: Rectangle {
                    color: "#1c1f1c"
                    radius: 8
                }
            }
        }
    }
    Component.onCompleted: open()
}
