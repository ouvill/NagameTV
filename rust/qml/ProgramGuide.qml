import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    required property string programsJson
    required property string status
    required property string channel
    signal refreshRequested()
    signal closeRequested()
    color: "#24282e"
    ColumnLayout {
        anchors.fill: parent; anchors.margins: 10
        RowLayout {
            Label { text: "番組表 — " + channel; color: "white"; Layout.fillWidth: true; elide: Text.ElideRight }
            Button { text: "更新"; onClicked: refreshRequested() }
            Button { text: "閉じる"; onClicked: closeRequested() }
        }
        Label { text: status; color: "#cccccc" }
        ListView {
            id: programs
            Layout.fillWidth: true; Layout.fillHeight: true
            clip: true; spacing: 8
            model: JSON.parse(programsJson)
            delegate: Rectangle {
                required property var modelData
                width: programs.width; height: content.implicitHeight + 16
                color: "#333940"
                Column {
                    id: content
                    anchors { left: parent.left; right: parent.right; top: parent.top; margins: 8 }
                    spacing: 4
                    Label {
                        width: parent.width; color: "#a6caff"
                        text: Qt.formatDateTime(new Date(modelData.startAt), "MM/dd hh:mm")
                            + " – " + Qt.formatDateTime(new Date(modelData.startAt + modelData.duration), "hh:mm")
                    }
                    Label { width: parent.width; color: "white"; text: modelData.name.length ? modelData.name : "番組名未取得"; textFormat: Text.PlainText; wrapMode: Text.Wrap; font.bold: true }
                    Label { width: parent.width; color: "#dddddd"; text: modelData.description; textFormat: Text.PlainText; wrapMode: Text.Wrap; maximumLineCount: 4; elide: Text.ElideRight }
                }
            }
            Label { anchors.centerIn: parent; visible: programs.count === 0; text: "表示できる番組がありません"; color: "white" }
            ScrollBar.vertical: ScrollBar {}
        }
    }
}
