pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: popup
    required property real windowWidth
    required property real windowHeight
    property bool playing: false
    property string tracksJson: "[]"
    property string errorText: ""
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property var tracks: JSON.parse(tracksJson)
    signal refreshRequested
    signal selectRequested(string trackId)
    parent: Overlay.overlay
    width: Math.min(380, windowWidth - 40)
    height: Math.min(340, windowHeight - 80)
    x: 24
    y: Math.max(20, windowHeight - height - 100)
    padding: 20
    modal: false
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    onAboutToShow: {
        errorText = "";
        refreshRequested();
    }
    onClosed: tracksJson = "[]"
    Timer {
        interval: 1000
        repeat: true
        running: popup.opened
        onTriggered: popup.refreshRequested()
    }
    background: Rectangle {
        radius: 18
        color: "#f21a1c1a"
        border.color: "#42ffffff"
    }
    contentItem: ColumnLayout {
        spacing: 12
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: "音声選択"
                color: "#f4f5f3"
                font.pixelSize: 17
                font.bold: true
                Layout.fillWidth: true
            }
            IconAction {
                iconSource: popup.iconDirectory + "x.svg"
                tip: "閉じる"
                implicitWidth: 28
                implicitHeight: 28
                onClicked: popup.close()
            }
        }
        ScrollView {
            id: scroll
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true
            ColumnLayout {
                width: scroll.availableWidth
                spacing: 6
                Repeater {
                    model: popup.tracks
                    delegate: TextAction {
                        id: option
                        objectName: "audioOption" + index
                        required property int index
                        required property var modelData
                        Layout.fillWidth: true
                        enabled: popup.playing && popup.tracks.length > 1
                        text: "音声 " + (index + 1) + (modelData.language ? " · " + modelData.language : "") + (modelData.title ? " · " + modelData.title : "")
                        contentItem: Label {
                            text: option.text
                            textFormat: Text.PlainText
                            color: "#f4f5f3"
                            font.pixelSize: 12
                            wrapMode: Text.Wrap
                        }
                        background: Rectangle {
                            radius: 12
                            color: option.modelData.selected ? "#389caf9f" : "#1c1f1c"
                            border.color: option.modelData.selected || option.visualFocus ? "#9caf9f" : "#28ffffff"
                        }
                        onClicked: popup.selectRequested(modelData.id)
                    }
                }
                Label {
                    visible: popup.tracks.length === 0
                    Layout.fillWidth: true
                    text: "音声トラックはまだありません。"
                    color: "#b6bab6"
                    wrapMode: Text.Wrap
                    font.pixelSize: 12
                }
            }
        }
        Label {
            Layout.fillWidth: true
            visible: popup.tracks.length === 1
            text: "この放送の音声は1つです。"
            color: "#b6bab6"
            wrapMode: Text.Wrap
            font.pixelSize: 11
        }
        Label {
            Layout.fillWidth: true
            visible: popup.errorText.length > 0
            text: popup.errorText
            textFormat: Text.PlainText
            color: "#f4f5f3"
            wrapMode: Text.Wrap
            font.pixelSize: 12
        }
    }
}
