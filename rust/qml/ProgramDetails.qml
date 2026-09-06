import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: popup
    required property var program
    property bool watchEnabled: false
    property string watchError: ""
    property double now: Date.now()
    readonly property bool live: watchEnabled && !!program && typeof program.watchKey === "string"
        && program.startAt <= now && now < program.startAt + program.duration
    signal watchRequested(string key)
    Timer { interval: 1000; repeat: true; running: popup.opened && popup.watchEnabled; onTriggered: popup.now = Date.now() }
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(560, parent ? parent.width - 32 : 560)
    height: Math.min(440, parent ? parent.height - 32 : 440)
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    Component.onCompleted: open()
    contentItem: ColumnLayout {
        RowLayout {
            Label { text: "番組詳細"; Layout.fillWidth: true; font.bold: true }
            Button { objectName: "closeProgramDetails"; text: "閉じる"; onClicked: popup.close() }
        }
        ScrollView {
            id: scroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true
            Column {
                width: scroll.availableWidth
                spacing: 12
                Label {
                    objectName: "programTitle"
                    width: parent.width
                    text: popup.program ? (popup.program.name || "番組名未取得") : "現在の番組情報がありません"
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    font.bold: true
                }
                Label {
                    width: parent.width
                    wrapMode: Text.Wrap
                    text: popup.program ? Qt.formatDateTime(new Date(popup.program.startAt), "MM/dd hh:mm")
                        + " – " + Qt.formatDateTime(new Date(popup.program.startAt + popup.program.duration), "hh:mm") : ""
                }
                Label {
                    objectName: "programDescription"
                    width: parent.width
                    text: popup.program ? (popup.program.description || "番組の説明はありません") : ""
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
            }
        }
        Label { objectName: "watchGuideError"; visible: popup.watchError !== ""; text: popup.watchError; textFormat: Text.PlainText; wrapMode: Text.Wrap; Layout.fillWidth: true; color: "#ffb4ab" }
        Button {
            id: watchButton
            objectName: "watchGuideProgram"
            visible: popup.live
            Layout.preferredWidth: 168; Layout.preferredHeight: 44
            text: "この番組を見る"
            background: Rectangle { radius: 22; color: "#9caf9f" }
            contentItem: Label { text: watchButton.text; color: "#17201a"; font.bold: true; horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter }
            onClicked: popup.watchRequested(popup.program.watchKey)
        }
    }
}
