import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: popup
    required property var program
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(560, parent ? parent.width - 32 : 560)
    height: Math.min(440, parent ? parent.height - 32 : 440)
    modal: true
    focus: true
    padding: Theme.spaceXl
    background: PanelSurface {}
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    Component.onCompleted: open()
    contentItem: ColumnLayout {
        RowLayout {
            Label { text: qsTranslate("Viewer", "Program details"); Layout.fillWidth: true; font.bold: true; color: Theme.textPrimary; font.pixelSize: Theme.fontHeading }
            ActionButton { objectName: "closeProgramDetails"; text: qsTranslate("Main", "Close"); onClicked: popup.close() }
        }
        ScrollView {
            id: scroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true
            Column {
                width: scroll.availableWidth
                spacing: Theme.spaceMd
                Label {
                    objectName: "programTitle"
                    color: Theme.textPrimary
                    font.pixelSize: Theme.fontTitle
                    width: parent.width
                    text: popup.program ? (popup.program.name || qsTranslate("Viewer", "Program title unavailable")) : qsTranslate("Viewer", "No current program information")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    font.bold: true
                }
                Label {
                    width: parent.width
                    color: Theme.textSecondary
                    font.pixelSize: Theme.fontCaption
                    wrapMode: Text.Wrap
                    text: popup.program ? Qt.formatDateTime(new Date(popup.program.startAt), "MM/dd hh:mm")
                        + " – " + Qt.formatDateTime(new Date(popup.program.startAt + popup.program.duration), "hh:mm") : ""
                }
                Label {
                    objectName: "programDescription"
                    color: Theme.textPrimary
                    font.pixelSize: Theme.fontBody
                    width: parent.width
                    text: popup.program ? (popup.program.description || qsTranslate("Viewer", "No program description")) : ""
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
                ProgramMetadata { width: parent.width; program: popup.program }
            }
        }
    }
}
