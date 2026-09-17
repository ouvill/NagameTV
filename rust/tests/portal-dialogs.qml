import QtQuick
import QtQuick.Dialogs

Window {
    id: root
    visible: true
    width: 640
    height: 480
    property bool painted: false
    property int accepted: 0
    property int rejected: 0
    property url fileUrl
    property url folderUrl
    onFrameSwapped: painted = true
    FileDialog {
        id: file
        title: "Recording portal integration test"
        fileMode: FileDialog.OpenFile
        nameFilters: ["Transport streams (*.ts *.TS *.m2ts *.M2TS)", "All files (*)"]
        onAccepted: { root.fileUrl = selectedFile; root.accepted++; }
        onRejected: root.rejected++
    }
    FolderDialog {
        id: folder
        title: "Screenshot folder portal integration test"
        onAccepted: { root.folderUrl = selectedFolder; root.accepted++; }
        onRejected: root.rejected++
    }
}
