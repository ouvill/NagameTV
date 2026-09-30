pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import MinimalViewer

Dialog {
    id: root
    objectName: "recordingFiles"
    required property VideoFileModel files
    property string recordedId: ""
    signal fileChosen(string recordedId, string videoId)
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(600, parent.width - Theme.spaceXl * 2)
    modal: true
    title: qsTranslate("RecordingLibrary", "Choose a video file")
    padding: Theme.spaceLg
    background: PanelSurface {}
    header: Label {
        text: root.title
        padding: Theme.spaceXl
        color: Theme.textPrimary
        font.pixelSize: Theme.fontHeading
    }
    onOpened: { list.currentIndex = 0; list.forceActiveFocus(Qt.TabFocusReason); }
    Connections {
        target: root.files
        function onChanged() { if (root.files.count === 0) root.close(); }
    }
    contentItem: ListView {
        id: list
        objectName: "recordingFileList"
        implicitHeight: Math.min(contentHeight, root.parent.height * 0.6)
        clip: true
        spacing: Theme.spaceSm
        model: root.files
        keyNavigationEnabled: false
        function choose(event) {
            if (event.isAutoRepeat) return;
            const button = currentItem as ActionButton;
            if (button) button.click();
        }
        Keys.onUpPressed: if (currentIndex > 0) decrementCurrentIndex()
        Keys.onDownPressed: {
            if (currentIndex + 1 < count) incrementCurrentIndex();
            else cancelButton.forceActiveFocus(Qt.TabFocusReason);
        }
        Keys.onReturnPressed: function(event) { choose(event); }
        Keys.onEnterPressed: function(event) { choose(event); }
        ScrollBar.vertical: ScrollBar {}
        delegate: ActionButton {
            id: choice
            required property string videoId
            required property string displayName
            required property string fileName
            required property bool originalTs
            objectName: "recordingFileChoice"
            width: list.width
            textAlignment: Text.AlignLeft
            selected: ListView.isCurrentItem && list.activeFocus
            text: (choice.originalTs ? qsTranslate("RecordingLibrary", "Original TS") : choice.displayName || choice.fileName || qsTranslate("RecordingLibrary", "Encoded video"))
                  + (choice.fileName.length && (choice.originalTs || choice.displayName.length) ? " · " + choice.fileName : "")
            onClicked: { root.fileChosen(root.recordedId, choice.videoId); root.close(); }
        }
    }
    footer: Item {
        implicitHeight: cancelButton.implicitHeight + Theme.spaceLg * 2
        ActionButton {
            id: cancelButton
            objectName: "recordingFilesCancel"
            anchors { right: parent.right; margins: Theme.spaceLg; verticalCenter: parent.verticalCenter }
            text: qsTranslate("Recording", "Cancel")
            onClicked: root.reject()
            Keys.onUpPressed: { list.currentIndex = list.count - 1; list.forceActiveFocus(Qt.TabFocusReason); }
        }
    }
}
