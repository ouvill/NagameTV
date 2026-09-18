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
    // JP7080382 / JP7277651 / JP7153786 require a separate list; JP7852687
    // requires a received-comment field to the video's right. Normal builds
    // omit CommentList.qml and show controls even with danmaku off, removing
    // that display feature rather than relying on simultaneous-use wording.
    // Estimated expiry 2027-03-02; registry status unverified, no auto-reactivation.
    // Player supplies the compile-time evaluation override, never a setting.
    readonly property bool showCommentControls: !evaluationCommentList
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
                    visible: root.recording && root.program && root.program.genres && root.program.genres.length > 0
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
            id: commentList
            Layout.fillWidth: true
            Layout.fillHeight: true
            objectName: "sidebarCommentList"
            active: root.page === ProgramSidebar.Comments && root.evaluationCommentList
            visible: active
            // Load only the evaluation resource. A static CommentList reference
            // would require registering it in normal builds as well.
            function configureSource() {
                if (!root.evaluationCommentList) {
                    source = "";
                    return;
                }
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
        ScrollView {
            id: commentControls
            objectName: "sidebarCommentControls"
            visible: root.page === ProgramSidebar.Comments && root.showCommentControls
            Layout.fillWidth: true
            Layout.fillHeight: true
            contentWidth: availableWidth
            clip: true
            ColumnLayout {
                width: commentControls.availableWidth
                spacing: 18
                Label {
                    objectName: "sidebarCommentStatus"
                    Layout.fillWidth: true
                    text: qsTranslate("Settings", "Live comments: %1").arg(root.commentStatus)
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: "#b6bab6"
                }
                CommentPresentation {
                    Layout.fillWidth: true
                    enabled: root.commentsEnabled
                    displayMode: root.displayMode
                    placementMode: root.placementMode
                    evaluationCollision: root.evaluationCollision
                    onSelected: function(display, placement) { root.presentationRequested(display, placement); }
                }
                DanmakuAdjustments {
                    Layout.fillWidth: true
                    enabled: root.commentsEnabled
                    textSize: root.textSize
                    textOpacity: root.textOpacity
                    speed: root.speed
                    onAdjusted: function(size, opacity, speed) { root.adjusted(size, opacity, speed); }
                }
                RowLayout {
                    Layout.fillWidth: true
                    enabled: root.commentsEnabled
                    Label { text: qsTranslate("Settings", "Drop shadow"); color: "#f4f5f3"; Layout.fillWidth: true }
                    ToggleSwitch {
                        text: qsTranslate("Settings", "Drop shadow")
                        checked: root.shadowEnabled
                        onToggled: root.shadowRequested(checked)
                    }
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
