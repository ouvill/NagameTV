pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root
    enum Page {
        Program,
        Channels,
        Comments
    }
    property bool danmakuEnabled: false
    property bool commentsEnabled: false
    signal danmakuRequested(bool enabled)
    property var commentModel: null
    property string commentProgramTitle: ""
    property string commentStatus: ""
    property int page: ProgramSidebar.Program
    property var channelRows: []
    property int selectedChannel: -1
    property string channelVisibility: "[]"
    property string activityJson: "[]"
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
    readonly property var program: JSON.parse(programJson)
    signal closeRequested
    color: "#151715"
    clip: true
    Rectangle {
        width: 1
        height: parent.height
        color: "#20ffffff"
    }
    WindowDragArea {
        anchors { left: parent.left; right: parent.right; top: parent.top }
        height: 76
        targetWindow: root.targetWindow
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
                tip: qsTranslate("Main", "Collapse")
                onClicked: root.closeRequested()
            }
            Label {
                text: root.page === ProgramSidebar.Comments ? qsTranslate("Main", "Comments") : (root.page === ProgramSidebar.Channels ? qsTranslate("Main", "Channels") : qsTranslate("Main", "Program information"))
                color: "#f4f5f3"
                font.pixelSize: 17
                font.bold: true
                Layout.fillWidth: true
            }
            Row {
                visible: root.page === ProgramSidebar.Comments
                enabled: root.commentsEnabled
                spacing: 9
                Layout.alignment: Qt.AlignVCenter
                Label {
                    height: parent.height
                    verticalAlignment: Text.AlignVCenter
                    text: qsTranslate("Main", "Danmaku")
                    color: root.danmakuEnabled ? "#f4f5f3" : "#b6bab6"
                    font.pixelSize: 13
                }
                ToggleSwitch {
                    objectName: "sidebarDanmakuToggle"
                    text: qsTranslate("Main", "Danmaku")
                    checked: root.danmakuEnabled
                    onToggled: root.danmakuRequested(checked)
                }
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
                        text: root.channelLabel.replace(/^\d+\s+/, "") || qsTranslate("Main", "Channels")
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
                    text: root.program ? (root.program.name || qsTranslate("Viewer", "Program title unavailable")) : qsTranslate("Main", "No program information")
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
                    text: qsTranslate("Main", "Summary")
                    color: "#b6bab6"
                    font.weight: Font.DemiBold
                }
                Label {
                    objectName: "programDescription"
                    Layout.fillWidth: true
                    text: root.program ? (root.program.description || qsTranslate("Viewer", "No program description")) : qsTranslate("Viewer", "No program description")
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
                    text: qsTranslate("Viewer", "Program information provided by Mirakurun")
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
                activityJson: root.activityJson
                selected: root.selectedChannel
                programsJson: root.channelPrograms
                visibilityJson: root.channelVisibility
                now: root.now
                onSelectRequested: function (index) {
                    root.selectRequested(index);
                }
            }
        }
        ColumnLayout {
            visible: root.page === ProgramSidebar.Comments
            Layout.fillWidth: true
            spacing: 5
            Label {
                text: qsTranslate("Viewer", "NX-Jikkyo program")
                color: "#929497"
                font.pixelSize: 12
            }
            Label {
                objectName: "commentProgramTitle"
                Layout.fillWidth: true
                text: root.commentProgramTitle || qsTranslate("Viewer", "Program title unavailable")
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
                maximumLineCount: 3
                elide: Text.ElideRight
                color: "#e5e5e4"
                font.pixelSize: 14
                ToolTip.visible: titleHover.hovered && root.commentProgramTitle.length > 0
                ToolTip.text: root.commentProgramTitle
                HoverHandler { id: titleHover }
            }
        }
        Loader {
            Layout.fillWidth: true
            Layout.fillHeight: true
            active: root.page === ProgramSidebar.Comments
            visible: active
            sourceComponent: CommentList {
                commentModel: root.commentModel
                status: root.commentStatus
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
                iconSource: root.iconDirectory + "message-square.svg"
                selected: root.page === ProgramSidebar.Comments
                text: qsTranslate("Main", "Comments")
                onClicked: root.pageRequested(ProgramSidebar.Comments)
            }
            SidebarTab {
                Layout.fillWidth: true
                iconSource: root.iconDirectory + "info.svg"
                selected: root.page === ProgramSidebar.Program
                text: qsTranslate("Main", "Program information")
                onClicked: root.pageRequested(ProgramSidebar.Program)
            }
            SidebarTab {
                Layout.fillWidth: true
                iconSource: root.iconDirectory + "grid-2x2.svg"
                selected: root.page === ProgramSidebar.Channels
                text: qsTranslate("Main", "Channels")
                onClicked: root.pageRequested(ProgramSidebar.Channels)
            }
        }
    }
}
