import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var program
    required property real now
    readonly property real progress: program && program.duration > 0 ? Math.max(0, Math.min(1, (now - program.startAt) / program.duration)) : 0
    Label {
        objectName: "cardProgramTitle"
        Layout.fillWidth: true
        text: root.program ? (root.program.name || "番組名未取得") : "現在の番組情報がありません"
        color: "#eeeeee"
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        maximumLineCount: 2
        elide: Text.ElideRight
    }
    Label {
        Layout.fillWidth: true
        text: root.program ? Qt.formatDateTime(new Date(root.program.startAt), "hh:mm") + " – " + Qt.formatDateTime(new Date(root.program.startAt + root.program.duration), "hh:mm") : ""
        color: "#bbbbbb"
    }
    ProgressBar {
        Layout.fillWidth: true
        visible: root.program !== null
        from: 0
        to: 1
        value: root.progress
    }
}
