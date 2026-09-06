pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property string programJson
    required property real progress
    readonly property var program: JSON.parse(programJson)
    property bool showDetails: false
    spacing: 2
    RowLayout {
        Button {
            id: currentButton
            text: root.program ? (root.program.name || "番組名未取得") : "現在の番組情報がありません"
            objectName: "currentProgramButton"
            Layout.fillWidth: true
            enabled: root.program !== null
            onClicked: root.showDetails = true
            contentItem: Label {
                text: currentButton.text
                textFormat: Text.PlainText
                elide: Text.ElideRight
            }
        }
        Label {
            text: root.program ? Qt.formatDateTime(new Date(root.program.startAt), "hh:mm")
                + " – " + Qt.formatDateTime(new Date(root.program.startAt + root.program.duration), "hh:mm") : ""
            color: "#cccccc"
        }
    }
    ProgressBar {
        objectName: "programProgress"
        Layout.fillWidth: true
        from: 0; to: 1
        value: root.progress
        visible: root.program !== null
    }
    Loader {
        objectName: "programDetailsLoader"
        active: root.showDetails
        sourceComponent: ProgramDetails {
            program: root.program
            onClosed: root.showDetails = false
        }
    }
}
