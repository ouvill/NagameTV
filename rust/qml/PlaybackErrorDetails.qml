import QtQuick
import MinimalViewer
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
    padding: Theme.spaceXl
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: PanelSurface {}
    contentItem: ColumnLayout {
        spacing: Theme.spaceMd
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: qsTranslate("Main", "Error details")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontHeading
                Layout.fillWidth: true
            }
            ActionButton {
                text: qsTranslate("Main", "Close")
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
                color: Theme.textSecondary
                font.pixelSize: Theme.fontCaption
                background: Rectangle {
                    color: Theme.surfaceRaised
                    radius: Theme.controlRadius
                }
            }
        }
    }
    Component.onCompleted: open()
}
