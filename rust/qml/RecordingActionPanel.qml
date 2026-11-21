pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MinimalViewer

Popup {
    id: root
    objectName: "recordingActionPanel"
    property real rowY: 0
    property string programName: ""
    property string channelAndTime: ""
    property string description: ""
    property bool started: false
    required property bool canPlay
    required property bool canChooseFile
    required property bool deletionEnabled
    required property bool canDelete
    property bool showingDetails: false
    signal playRequested
    signal fileRequested
    signal deleteRequested

    parent: Overlay.overlay
    x: Math.max(Theme.spaceLg, parent.width - width - Theme.spaceLg)
    y: Math.max(Theme.spaceLg, Math.min(rowY, parent.height - height - Theme.spaceLg))
    width: Math.min(showingDetails ? 560 : 332, parent.width - 2 * Theme.spaceLg)
    padding: Theme.spaceLg
    modal: true
    focus: true
    closePolicy: Popup.CloseOnPressOutside
    background: PanelSurface {}
    onOpened: {
        const first = play.enabled ? play : chooseFile.enabled ? chooseFile : details;
        first.forceActiveFocus(Qt.TabFocusReason);
    }
    onClosed: showingDetails = false

    contentItem: FocusScope {
        implicitHeight: root.showingDetails ? detailColumn.implicitHeight : buttons.implicitHeight
        Keys.onPressed: function(event) {
            if (event.key !== Qt.Key_Back && event.key !== Qt.Key_Escape && event.key !== Qt.Key_Left) return;
            event.accepted = true;
            if (!event.isAutoRepeat) root.close();
        }
        DirectionalFocus {
            anchors.fill: parent
            visible: !root.showingDetails
            navigationItems: [play, chooseFile, details, deleteAction]
            onBoundaryReached: function(key) { if (key === Qt.Key_Left) root.close(); }
            ColumnLayout {
                id: buttons
                anchors.fill: parent
                spacing: Theme.spaceSm
                Label {
                    Layout.fillWidth: true
                    text: root.programName
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    maximumLineCount: 2
                    elide: Text.ElideRight
                    color: Theme.textPrimary
                    font.pixelSize: Theme.fontControl
                    font.bold: true
                }
                ActionButton {
                    id: play
                    objectName: "recordingActionPlay"
                    Layout.fillWidth: true
                    enabled: root.canPlay
                    textAlignment: Text.AlignLeft
                    text: root.started ? qsTranslate("RecordingLibrary", "Play from the beginning") : qsTranslate("RecordingLibrary", "Play")
                    onClicked: root.playRequested()
                }
                ActionButton {
                    id: chooseFile
                    objectName: "recordingActionChooseFile"
                    Layout.fillWidth: true
                    enabled: root.canChooseFile
                    textAlignment: Text.AlignLeft
                    text: qsTranslate("RecordingLibrary", "Choose file to play")
                    onClicked: root.fileRequested()
                }
                ActionButton {
                    id: details
                    objectName: "recordingActionDetails"
                    Layout.fillWidth: true
                    textAlignment: Text.AlignLeft
                    text: qsTranslate("Viewer", "Program details")
                    onClicked: {
                        root.showingDetails = true;
                        descriptionView.contentY = 0;
                        descriptionView.forceActiveFocus(Qt.TabFocusReason);
                    }
                }
                ActionButton {
                    id: deleteAction
                    objectName: "recordingActionDelete"
                    Layout.fillWidth: true
                    visible: root.deletionEnabled
                    enabled: root.canDelete
                    textAlignment: Text.AlignLeft
                    text: qsTranslate("RecordingLibrary", "Delete…")
                    onClicked: root.deleteRequested()
                }
            }
        }
        ColumnLayout {
            id: detailColumn
            anchors.fill: parent
            visible: root.showingDetails
            spacing: Theme.spaceMd
            Label {
                Layout.fillWidth: true
                text: root.programName
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Theme.textPrimary
                font.pixelSize: Theme.fontHeading
                font.bold: true
            }
            Label {
                Layout.fillWidth: true
                text: root.channelAndTime
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Theme.textSecondary
                font.pixelSize: Theme.fontBody
            }
            Flickable {
                id: descriptionView
                objectName: "recordingActionDescriptionView"
                Layout.fillWidth: true
                Layout.preferredHeight: Math.min(descriptionText.implicitHeight, root.parent.height / 2)
                contentWidth: width
                contentHeight: descriptionText.implicitHeight
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                Keys.onUpPressed: contentY = Math.max(0, contentY - Theme.controlHeight)
                Keys.onDownPressed: contentY = Math.min(Math.max(0, contentHeight - height), contentY + Theme.controlHeight)
                ScrollBar.vertical: ScrollBar {}
                Label {
                    id: descriptionText
                    objectName: "recordingActionDescription"
                    width: descriptionView.width - Theme.spaceMd
                    text: root.description || qsTranslate("Viewer", "No program description")
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: Theme.textPrimary
                    font.pixelSize: Theme.fontBody
                }
            }
            ActionButton {
                Layout.alignment: Qt.AlignRight
                text: qsTranslate("RecordingLibrary", "Close")
                onClicked: root.close()
            }
        }
    }
}
