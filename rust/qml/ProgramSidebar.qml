pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

FocusScope {
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
    property string densityMode: "normal"
    property real textSize: 24
    property real textOpacity: 1
    property real speed: 1
    property bool shadowEnabled: true
    signal densityRequested(string densityMode)
    signal presentationRequested(string displayMode, string placementMode)
    signal adjusted(real textSize, real textOpacity, real speed)
    signal shadowRequested(bool enabled)
    property bool statsVisible: false
    signal statsRequested(bool visible)
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
    function enter() {
        if (page === ProgramSidebar.Playback) playback.enter();
        else if (page === ProgramSidebar.Channels && channelsLoader.item)
            (channelsLoader.item as SidebarChannels).focusBrowser();
        else scroll.forceActiveFocus(Qt.TabFocusReason);
    }
    function enterLast() {
        if (page === ProgramSidebar.Playback) playback.enterLast();
        else if (page === ProgramSidebar.Channels && channelsLoader.item)
            (channelsLoader.item as SidebarChannels).focusLast();
        else scroll.forceActiveFocus(Qt.TabFocusReason);
    }
    function choosePage(page: int, keyboard: bool) {
        pageRequested(page);
        if (keyboard) Qt.callLater(enter);
    }
    function scrollProgram(direction: int) {
        const flick = scroll.contentItem as Flickable;
        if (!flick) return;
        const limit = Math.max(0, flick.contentHeight - flick.height);
        if (direction < 0 && flick.contentY <= 0) closeButton.forceActiveFocus(Qt.TabFocusReason);
        else if (direction > 0 && flick.contentY >= limit) footer.enter();
        else flick.contentY = Math.max(0, Math.min(limit, flick.contentY + direction * Theme.controlHeight));
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
    clip: true
    Rectangle { anchors.fill: parent; color: Theme.surface }
    Rectangle {
        width: 1
        height: parent.height
        color: Theme.overlayBorder
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
        spacing: Theme.spaceLg
        RowLayout {
            Layout.fillWidth: true
            Label {
                objectName: "sidebarHeading"
                text: root.page === ProgramSidebar.Playback ? qsTranslate("Main", "Viewing settings")
                    : root.page === ProgramSidebar.Channels ? qsTranslate("Main", "Channels") : qsTranslate("Main", "Program information")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontTitle
                font.bold: true
                Layout.fillWidth: true
                elide: Text.ElideRight
            }
            IconAction {
                id: closeButton
                objectName: "sidebarCloseButton"
                Keys.onDownPressed: root.enter()
                Keys.onUpPressed: footer.enter()
                flat: true
                iconSource: root.iconDirectory + "panel-right-close.svg"
                tip: qsTranslate("Main", "Collapse")
                onClicked: root.closeRequested()
            }
        }
        ScrollView {
            id: scroll
            Keys.onUpPressed: root.scrollProgram(-1)
            Keys.onDownPressed: root.scrollProgram(1)
            Keys.onReturnPressed: function(event) { if (!event.isAutoRepeat) footer.enter(); }
            Keys.onEnterPressed: function(event) { if (!event.isAutoRepeat) footer.enter(); }
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
                        color: Theme.textPrimary
                        font.weight: Font.DemiBold
                        textFormat: Text.PlainText
                        Layout.fillWidth: true
                        wrapMode: Text.Wrap
                    }
                }
                Label {
                    objectName: "programTitle"
                    Layout.fillWidth: true
                    text: root.program ? (root.program.name || root.fallbackTitle || qsTranslate("Viewer", "Program title unavailable")) : root.programStatus === "pending" ? qsTranslate("Settings", "Acquiring program information…") : root.programStatus === "failed" ? qsTranslate("Settings", "Could not read program information") : qsTranslate("Main", "No program information")
                    color: Theme.textPrimary
                    font.pixelSize: Theme.fontTitle
                    font.bold: true
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                }
                Label {
                    Layout.fillWidth: true
                    text: root.program && root.program.startAt !== null && root.program.duration !== null ? Qt.formatDateTime(new Date(root.program.startAt), root.recording ? "yyyy/MM/dd hh:mm" : "hh:mm") + " – " + Qt.formatDateTime(new Date(root.program.startAt + root.program.duration), "hh:mm") : ""
                    color: Theme.textPrimary
                    font.pixelSize: Theme.fontCaption
                }
                Label {
                    visible: root.recording && !!root.program && !!root.program.genres && root.program.genres.length > 0
                    Layout.fillWidth: true
                    readonly property var names: [qsTranslate("Viewer", "News"), qsTranslate("Viewer", "Sports"), qsTranslate("Viewer", "Information"), qsTranslate("Viewer", "Drama"), qsTranslate("Viewer", "Music"), qsTranslate("Viewer", "Variety"), qsTranslate("Viewer", "Film"), qsTranslate("Viewer", "Animation"), qsTranslate("Viewer", "Documentary"), qsTranslate("Viewer", "Theater"), qsTranslate("Viewer", "Education"), qsTranslate("Viewer", "Welfare")]
                    text: visible ? root.program.genres.map(genre => names[genre[0]] || qsTranslate("Viewer", "Other")).join(" / ") : ""
                    wrapMode: Text.Wrap
                    color: Theme.textSecondary
                    font.pixelSize: Theme.fontCaption
                }
                ProgressBar {
                    Layout.fillWidth: true
                    visible: root.program && root.program.progressKnown !== false
                    value: root.progress
                    background: Rectangle {
                        implicitHeight: 3
                        radius: Theme.indicatorRadius
                        color: Theme.overlayBorder
                    }
                    contentItem: Item {
                        Rectangle {
                            width: parent.width * root.progress
                            height: 3
                            radius: Theme.indicatorRadius
                            color: Theme.accent
                        }
                    }
                }
                Pane {
                    objectName: "commentProgramCard"
                    visible: !root.recording
                    Layout.fillWidth: true
                    Layout.topMargin: 14
                    Layout.bottomMargin: 14
                    padding: Theme.spaceLg
                    background: Rectangle { radius: Theme.panelRadius; color: Theme.surfaceRaised }
                    contentItem: ColumnLayout {
                        spacing: Theme.spaceLg
                        Label {
                            objectName: "sidebarCommentStatus"
                            Layout.fillWidth: true
                            text: "NX-Jikkyo · " + root.commentStatus
                            textFormat: Text.PlainText
                            color: Theme.accent; font.pixelSize: Theme.fontCaption
                            wrapMode: Text.Wrap
                        }
                        Label {
                            objectName: "commentProgramTitle"
                            Layout.fillWidth: true
                            text: root.commentProgramTitle || qsTranslate("Viewer", "Program title unavailable")
                            textFormat: Text.PlainText
                            wrapMode: Text.Wrap
                            color: Theme.textPrimary; font.pixelSize: Theme.fontControl; font.bold: true
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
                    color: Theme.textSecondary
                    font.weight: Font.DemiBold
                }
                Label {
                    objectName: "programDescription"
                    Layout.fillWidth: true
                    text: root.program ? (root.program.description || qsTranslate("Viewer", "No program description")) : qsTranslate("Viewer", "No program description")
                    color: Theme.textPrimary
                    font.pixelSize: Theme.fontBody
                    wrapMode: Text.Wrap
                    textFormat: Text.PlainText
                    lineHeight: 1.35
                }
                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 1
                    color: Theme.overlayHover
                }
                Label {
                    Layout.fillWidth: true
                    text: root.recording ? qsTranslate("Viewer", "Program information from recording TS")
                        : root.program && root.program.source === "broadcast_ts" ? qsTranslate("Viewer", "Program information from broadcast TS")
                        : qsTranslate("Viewer", "Program information provided by Mirakurun")
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontCaption
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
                onBoundaryReached: function(key) {
                    if (key === Qt.Key_Up) closeButton.forceActiveFocus(Qt.TabFocusReason);
                    else footer.enter();
                }
                onSelectRequested: function (index) {
                    root.selectRequested(index);
                }
            }
        }
        PlaybackSettings {
            id: playback
            objectName: "sidebarPlaybackSettings"
            visible: root.page === ProgramSidebar.Playback
            Layout.fillWidth: true
            Layout.fillHeight: true
            commentsEnabled: root.commentsEnabled
            danmakuEnabled: root.danmakuEnabled
            displayMode: root.displayMode
            placementMode: root.placementMode
            densityMode: root.densityMode
            evaluationCollision: root.evaluationCollision
            textSize: root.textSize
            textOpacity: root.textOpacity
            speed: root.speed
            shadowEnabled: root.shadowEnabled
            statsVisible: root.statsVisible
            onBoundaryReached: function(key) {
                if (key === Qt.Key_Up) closeButton.forceActiveFocus(Qt.TabFocusReason);
                else footer.enter();
            }
            onDensityRequested: function(density) { root.densityRequested(density); }
            onPresentationRequested: function(display, placement) { root.presentationRequested(display, placement); }
            onDanmakuRequested: function(enabled) { root.danmakuRequested(enabled); }
            onAdjusted: function(size, opacity, speed) { root.adjusted(size, opacity, speed); }
            onShadowRequested: function(enabled) { root.shadowRequested(enabled); }
            onStatsRequested: function(visible) { root.statsRequested(visible); }
        }
        Rectangle {
            Layout.fillWidth: true
            Layout.preferredHeight: 1
            color: Theme.overlayHover
        }
        DirectionalFocus {
            id: footer
            Layout.fillWidth: true
            implicitHeight: tabs.implicitHeight
            navigationItems: [playbackTab, programTab, channelsTab]
            initialItem: root.page === ProgramSidebar.Playback ? playbackTab
                : root.page === ProgramSidebar.Program ? programTab : channelsTab
            onBoundaryReached: function(key) {
                if (key === Qt.Key_Up) root.enterLast();
                else if (key === Qt.Key_Down) closeButton.forceActiveFocus(Qt.TabFocusReason);
            }
            RowLayout {
                id: tabs
                anchors.fill: parent
                spacing: Theme.spaceSm
                SidebarTab {
                    id: playbackTab
                    Layout.fillWidth: true
                    objectName: "playbackSidebarTab"
                    iconSource: root.iconDirectory + "settings-2.svg"
                    selected: root.page === ProgramSidebar.Playback
                    text: qsTranslate("Main", "Viewing settings")
                    onClicked: root.choosePage(ProgramSidebar.Playback, focusVisible)
                }
                SidebarTab {
                    id: programTab
                    Layout.fillWidth: true
                    objectName: "programSidebarTab"
                    iconSource: root.iconDirectory + "info.svg"
                    selected: root.page === ProgramSidebar.Program
                    text: qsTranslate("Main", "Program information")
                    onClicked: root.choosePage(ProgramSidebar.Program, focusVisible)
                }
                SidebarTab {
                    id: channelsTab
                    Layout.fillWidth: true
                    objectName: "channelsSidebarTab"
                    iconSource: root.iconDirectory + "grid-2x2.svg"
                    selected: root.page === ProgramSidebar.Channels
                    text: qsTranslate("Main", "Channels")
                    onClicked: root.choosePage(ProgramSidebar.Channels, focusVisible)
                }
            }
        }
    }
}
