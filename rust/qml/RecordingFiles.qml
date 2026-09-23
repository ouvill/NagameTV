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
    onOpened: { list.currentIndex = 0; list.forceActiveFocus(); }
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
        keyNavigationEnabled: true
        Keys.onReturnPressed: { const button = list.currentItem as ActionButton; if (button) button.clicked(); }
        Keys.onEnterPressed: { const button = list.currentItem as ActionButton; if (button) button.clicked(); }
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
    footer: DialogButtonBox {
        standardButtons: Dialog.Cancel
        delegate: ActionButton {}
        background: Item {}
        padding: Theme.spaceLg
        onRejected: root.reject()
    }
}
