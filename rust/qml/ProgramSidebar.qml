pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root
    enum Page {
        Program,
        Channels
    }
    property int page: ProgramSidebar.Program
    property var channelRows: []
    property int selectedChannel: -1
    property string channelPrograms: "[]"
    property real now: 0
    signal pageRequested(int page)
    signal selectRequested(int index)
    required property string programJson
    required property Window targetWindow
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    property string channelLabel: ""
    property string logoUrl: ""
    property real progress: 0
    property bool guideEnabled: false
    readonly property var program: JSON.parse(programJson)
    signal closeRequested
    signal guideRequested
    signal settingsRequested
    color: "#151715"
    clip: true
    Rectangle {
        width: 1
        height: parent.height
        color: "#20ffffff"
    }
    Row {
        anchors {
            right: parent.right
            top: parent.top
            margins: 18
        }
        spacing: 10
        IconAction {
            iconSource: root.iconDirectory + "calendar-days.svg"
            tip: "番組表"
            enabled: root.guideEnabled
            onClicked: root.guideRequested()
        }
        IconAction {
            iconSource: root.iconDirectory + "settings-2.svg"
            tip: "設定"
            onClicked: root.settingsRequested()
        }
        WindowButtons {
            targetWindow: root.targetWindow
            iconDirectory: root.iconDirectory
        }
    }
    ColumnLayout {
        x: 24
        y: 78
        width: parent.width - 48
        height: parent.height - 102
        spacing: 14
        RowLayout {
            Layout.fillWidth: true
            IconAction {
                iconSource: root.iconDirectory + "panel-right-close.svg"
                tip: "閉じる"
                onClicked: root.closeRequested()
            }
            Label {
                text: root.page === ProgramSidebar.Channels ? "チャンネル" : "番組情報"
                color: "#f4f5f3"
                font.pixelSize: 17
                font.bold: true
                Layout.fillWidth: true
            }
        }
        ScrollView {
            id: scroll
            visible: root.page === ProgramSidebar.Program
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true
            ColumnLayout {
                width: scroll.availableWidth
                spacing: 13
                RowLayout {
                    Layout.fillWidth: true
                    ChannelLogo {
                        Layout.preferredWidth: 56
                        Layout.preferredHeight: 32
                        logoUrl: root.logoUrl
                    }
                    Label {
                        text: root.channelLabel.replace(/^\d+\s+/, "") || "チャンネル"
                        color: "#f4f5f3"
                        font.weight: Font.DemiBold
                        textFormat: Text.PlainText
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                    }
                }
                Label {
                    objectName: "programTitle"
                    Layout.fillWidth: true
                    text: root.program ? (root.program.name || "番組名未取得") : "番組情報なし"
                    color: "#f4f5f3"
                    font.pixelSize: 23
                    font.bold: true
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                }
                Label {
                    Layout.fillWidth: true
                    text: root.program ? Qt.formatDateTime(new Date(root.program.startAt), "hh:mm") + " – " + Qt.formatDateTime(new Date(root.program.startAt + root.program.duration), "hh:mm") : ""
                    color: "#d4d4d3"
                    font.pixelSize: 13
                }
                ProgressBar {
                    Layout.fillWidth: true
                    value: root.progress
                    background: Rectangle {
                        implicitHeight: 3
                        radius: 2
                        color: "#30ffffff"
                    }
                    contentItem: Item {
                        Rectangle {
                            width: parent.width * root.progress
                            height: 3
                            radius: 2
                            color: "#9caf9f"
                        }
                    }
                }
                Label {
                    text: "概要"
                    color: "#b6bab6"
                    font.weight: Font.DemiBold
                }
                Label {
                    objectName: "programDescription"
                    Layout.fillWidth: true
                    text: root.program ? (root.program.description || "番組の説明はありません") : "番組の説明はありません"
                    color: "#e4e4e3"
                    font.pixelSize: 15
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    lineHeight: 1.35
                }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 1
                    color: "#18ffffff"
                }
                Label {
                    Layout.fillWidth: true
                    text: "番組情報は Mirakurun より提供されています"
                    color: "#929497"
                    font.pixelSize: 12
                    wrapMode: Text.Wrap
                }
            }
        }
        Loader {
            Layout.fillWidth: true
            Layout.fillHeight: true
            active: root.page === ProgramSidebar.Channels
            visible: active
            sourceComponent: SidebarChannels {
                rows: root.channelRows
                selected: root.selectedChannel
                programsJson: root.channelPrograms
                now: root.now
                onSelectRequested: function (index) {
                    root.selectRequested(index);
                }
            }
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: "#18ffffff"
        }
        RowLayout {
            Layout.fillWidth: true
            spacing: 8
            SidebarTab {
                Layout.fillWidth: true
                iconSource: root.iconDirectory + "info.svg"
                selected: root.page === ProgramSidebar.Program
                text: "番組情報"
                onClicked: root.pageRequested(ProgramSidebar.Program)
            }
            SidebarTab {
                Layout.fillWidth: true
                iconSource: root.iconDirectory + "grid-2x2.svg"
                selected: root.page === ProgramSidebar.Channels
                text: "チャンネル"
                onClicked: root.pageRequested(ProgramSidebar.Channels)
            }
        }
    }
}
