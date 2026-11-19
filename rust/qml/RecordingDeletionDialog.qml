pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MinimalViewer

Dialog {
    id: root
    objectName: "recordingDeletionDialog"
    property string programName: ""
    property string channelName: ""
    property string dateLabel: ""
    required property string deletionState
    required property string errorText
    signal deleteRequested
    readonly property bool deleting: deletionState === "deleting"
    readonly property bool confirming: deletionState === "confirming"
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(600, parent.width - Theme.spaceXl * 2)
    modal: true
    focus: true
    closePolicy: Popup.NoAutoClose
    title: qsTranslate("RecordingLibrary", "Delete recording?")
    padding: Theme.spaceLg
    background: PanelSurface {}
    header: Label {
        text: root.title
        padding: Theme.spaceLg
        color: Theme.textPrimary
        font.pixelSize: Theme.fontHeading
    }
    onOpened: cancelButton.forceActiveFocus(Qt.TabFocusReason)
    onDeletionStateChanged: {
        if (!visible) return;
        if (deleting) contentItem.forceActiveFocus(Qt.OtherFocusReason);
        else if (deletionState === "failed") cancelButton.forceActiveFocus(Qt.TabFocusReason);
    }
    contentItem: FocusScope {
        implicitHeight: content.implicitHeight
        Keys.onPressed: function(event) {
            if (event.key !== Qt.Key_Back && event.key !== Qt.Key_Escape) return;
            event.accepted = true;
            if (!event.isAutoRepeat && !root.deleting) root.reject();
        }
        ColumnLayout {
            id: content
            anchors.left: parent.left
            anchors.right: parent.right
            spacing: Theme.spaceMd
            Label {
                objectName: "recordingDeletionProgram"
                Layout.fillWidth: true
                text: root.programName
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                maximumLineCount: 3
                elide: Text.ElideRight
                color: Theme.textPrimary
                font.pixelSize: Theme.fontControl
                font.bold: true
            }
            Label {
                objectName: "recordingDeletionChannelDate"
                Layout.fillWidth: true
                text: root.channelName + "  " + root.dateLabel
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: Theme.textSecondary
                font.pixelSize: Theme.fontBody
            }
            Label {
                objectName: "recordingDeletionMessage"
                Layout.fillWidth: true
                text: root.deleting ? qsTranslate("RecordingLibrary", "Deleting recording…")
                      : root.confirming ? qsTranslate("RecordingLibrary", "EPGStation will permanently remove this recording.")
                      : root.errorText
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                color: root.deletionState === "failed" ? Theme.error : Theme.textSecondary
                font.pixelSize: Theme.fontBody
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: Theme.spaceMd
                ActionButton {
                    id: cancelButton
                    objectName: "recordingDeletionCancel"
                    enabled: !root.deleting
                    text: root.confirming ? qsTranslate("Recording", "Cancel") : qsTranslate("RecordingLibrary", "Close")
                    onClicked: root.reject()
                    Keys.onRightPressed: if (deleteButton.visible) deleteButton.forceActiveFocus(Qt.TabFocusReason)
                    Keys.onDownPressed: if (deleteButton.visible) deleteButton.forceActiveFocus(Qt.TabFocusReason)
                }
                ActionButton {
                    id: deleteButton
                    objectName: "recordingDeletionConfirm"
                    visible: root.confirming
                    text: qsTranslate("RecordingLibrary", "Delete")
                    onClicked: root.deleteRequested()
                    Keys.onLeftPressed: cancelButton.forceActiveFocus(Qt.TabFocusReason)
                    Keys.onUpPressed: cancelButton.forceActiveFocus(Qt.TabFocusReason)
                }
            }
        }
    }
}
