pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root
    enum Page {
        Program,
        Channels,
        Playback
    }
    property bool recording: false
    property string fallbackTitle: ""
    property bool danmakuEnabled: false
    property bool commentsEnabled: false
    property bool evaluationCommentList: false
    property bool evaluationCollision: false
    property string displayMode: "scroll"
    property string placementMode: "sequential"
    property real textSize: 24
    property real textOpacity: 1
    property real speed: 1
    property bool shadowEnabled: true
    signal presentationRequested(string displayMode, string placementMode)
    signal adjusted(real textSize, real textOpacity, real speed)
    signal shadowRequested(bool enabled)
    property bool statsVisible: false
    signal statsRequested(bool visible)
    signal timeshiftSettingsRequested()
    signal danmakuRequested(bool enabled)
    property var commentModel: null
    property string commentProgramTitle: ""
    property string commentStatus: ""
    property int page: ProgramSidebar.Playback
    required property ChannelModel channelModel
    property int selectedChannel: -1
    property int viewingIndex: -1
    function openChannels() {
        const channels = channelsLoader.item as SidebarChannels;
        if (channels) channels.openBrowser();
    }
    property string channelVisibility: "[]"
    property string activityJson: "[]"
    property string channelPrograms: "[]"
    property real now: 0
    signal pageRequested(int page)
    signal selectRequested(int index)
    property string programStatus: "unavailable"
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
        y: 88
        width: parent.width - 48
        height: parent.height - 112
        spacing: 16
        RowLayout {
            Layout.fillWidth: true
            Label {
                objectName: "sidebarHeading"
                text: root.page === ProgramSidebar.Playback ? qsTranslate("Main", "Playback settings")
                    : root.page === ProgramSidebar.Channels ? qsTranslate("Main", "Channels") : qsTranslate("Main", "Program information")
                color: "#f4f5f3"
                font.pixelSize: 22
                font.bold: true
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
            IconAction {
                objectName: "sidebarCloseButton"
                flat: true
                iconSource: root.iconDirectory + "panel-right-close.svg"
                tip: qsTranslate("Main", "Collapse")
                onClicked: root.closeRequested()
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
                    text: root.program ? (root.program.name || root.fallbackTitle || qsTranslate("Viewer", "Program title unavailable")) : root.programStatus === "pending" ? qsTranslate("Viewer", "Acquiring program information…") : root.programStatus === "failed" ? qsTranslate("Viewer", "Could not read program information") : qsTranslate("Main", "No program information")
                    color: "#f4f5f3"
                    font.pixelSize: 23
                    font.bold: true
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                }
                Label {
                    Layout.fillWidth: true
                    text: root.program && root.program.startAt !== null && root.program.duration !== null ? Qt.formatDateTime(new Date(root.program.startAt), root.recording ? "yyyy/MM/dd hh:mm" : "hh:mm") + " – " + Qt.formatDateTime(new Date(root.program.startAt + root.program.duration), "hh:mm") : ""
                    color: "#d4d4d3"
                    font.pixelSize: 13
                }
                Label {
                    visible: root.recording && !!root.program && !!root.program.genres && root.program.genres.length > 0
                    Layout.fillWidth: true
                    readonly property var names: [qsTranslate("Viewer", "News"), qsTranslate("Viewer", "Sports"), qsTranslate("Viewer", "Information"), qsTranslate("Viewer", "Drama"), qsTranslate("Viewer", "Music"), qsTranslate("Viewer", "Variety"), qsTranslate("Viewer", "Film"), qsTranslate("Viewer", "Animation"), qsTranslate("Viewer", "Documentary"), qsTranslate("Viewer", "Theater"), qsTranslate("Viewer", "Education"), qsTranslate("Viewer", "Welfare")]
                    text: visible ? root.program.genres.map(genre => names[genre[0]] || qsTranslate("Viewer", "Other")).join(" / ") : ""
                    wrapMode: Text.Wrap
                    color: "#b6bab6"
                    font.pixelSize: 12
                }
                ProgressBar {
                    Layout.fillWidth: true
                    visible: root.program && root.program.progressKnown !== false
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
                Pane {
                    objectName: "commentProgramCard"
                    visible: !root.recording
                    Layout.fillWidth: true
                    Layout.topMargin: 14
                    Layout.bottomMargin: 14
                    padding: 16
                    background: Rectangle { radius: 12; color: "#1c201d" }
                    contentItem: ColumnLayout {
                        spacing: 16
                        Label {
                            objectName: "sidebarCommentStatus"
                            Layout.fillWidth: true
                            text: "NX-Jikkyo · " + root.commentStatus
                            textFormat: Text.PlainText
                            color: "#9caf9f"; font.pixelSize: 12
                            wrapMode: Text.Wrap
                        }
                        Label {
                            objectName: "commentProgramTitle"
                            Layout.fillWidth: true
                            text: root.commentProgramTitle || qsTranslate("Viewer", "Program title unavailable")
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            color: "#f4f5f3"; font.pixelSize: 16; font.bold: true
                        }
                    }
                }
                // The received-comment list remains exclusive to evaluation builds.
                Loader {
                    id: commentList
                    objectName: "sidebarCommentList"
                    Layout.fillWidth: true
                    Layout.preferredHeight: active ? 240 : 0
                    active: root.page === ProgramSidebar.Program && root.evaluationCommentList
                    visible: active
                    function configureSource() {
                        if (!root.evaluationCommentList) { source = ""; return; }
                        setSource(Qt.resolvedUrl("CommentList.qml"), {
                            commentModel: Qt.binding(() => root.commentModel),
                            status: Qt.binding(() => root.commentStatus)
                        });
                    }
                    Component.onCompleted: configureSource()
                    Connections {
                        target: root
                        function onEvaluationCommentListChanged() { commentList.configureSource(); }
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
                    text: root.recording ? qsTranslate("Viewer", "Program information from recording TS")
                        : root.program && root.program.source === "broadcast_ts" ? qsTranslate("Viewer", "Program information from broadcast TS")
                        : qsTranslate("Viewer", "Program information provided by Mirakurun")
                    color: "#929497"
                    font.pixelSize: 12
                    wrapMode: Text.Wrap
                }
            }
        }
        Loader {
            id: channelsLoader
            Layout.fillWidth: true
            Layout.fillHeight: true
            active: root.page === ProgramSidebar.Channels
            visible: active
            sourceComponent: SidebarChannels {
                channels: root.channelModel
                activityJson: root.activityJson
                selected: root.selectedChannel
                viewingIndex: root.viewingIndex
                programsJson: root.channelPrograms
                visibilityJson: root.channelVisibility
                now: root.now
                onSelectRequested: function (index) {
                    root.selectRequested(index);
                }
            }
        }
        PlaybackSettings {
            objectName: "sidebarPlaybackSettings"
            visible: root.page === ProgramSidebar.Playback
            Layout.fillWidth: true
            Layout.fillHeight: true
            commentsEnabled: root.commentsEnabled
            danmakuEnabled: root.danmakuEnabled
            displayMode: root.displayMode
            placementMode: root.placementMode
            evaluationCollision: root.evaluationCollision
            textSize: root.textSize
            textOpacity: root.textOpacity
            speed: root.speed
            shadowEnabled: root.shadowEnabled
            statsVisible: root.statsVisible
            onPresentationRequested: function(display, placement) { root.presentationRequested(display, placement); }
            onDanmakuRequested: function(enabled) { root.danmakuRequested(enabled); }
            onAdjusted: function(size, opacity, speed) { root.adjusted(size, opacity, speed); }
            onShadowRequested: function(enabled) { root.shadowRequested(enabled); }
            onStatsRequested: function(visible) { root.statsRequested(visible); }
            onTimeshiftSettingsRequested: root.timeshiftSettingsRequested()
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
                objectName: "playbackSidebarTab"
                iconSource: root.iconDirectory + "settings-2.svg"
                selected: root.page === ProgramSidebar.Playback
                text: qsTranslate("Main", "Playback settings")
                onClicked: root.pageRequested(ProgramSidebar.Playback)
            }
            SidebarTab {
                Layout.fillWidth: true
                objectName: "programSidebarTab"
                iconSource: root.iconDirectory + "info.svg"
                selected: root.page === ProgramSidebar.Program
                text: qsTranslate("Main", "Program information")
                onClicked: root.pageRequested(ProgramSidebar.Program)
            }
            SidebarTab {
                Layout.fillWidth: true
                objectName: "channelsSidebarTab"
                iconSource: root.iconDirectory + "grid-2x2.svg"
                selected: root.page === ProgramSidebar.Channels
                text: qsTranslate("Main", "Channels")
                onClicked: root.pageRequested(ProgramSidebar.Channels)
            }
        }
    }
}
