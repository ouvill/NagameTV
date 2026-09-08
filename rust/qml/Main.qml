import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.freedesktop.gstreamer.Qt6GLVideoItem 1.0
import MinimalViewer 1.0

ApplicationWindow {
    id: root
    visible: true
    flags: Qt.Window | Qt.FramelessWindowHint
    width: 1440
    height: 900
    minimumWidth: 900
    minimumHeight: 560
    title: programIdentity.item && programIdentity.item.program && programIdentity.item.program.name
        ? programIdentity.item.program.name : qsTranslate("Main", "Mirakurun Viewer")
    color: "#0b0c0b"
    font.family: "Noto Sans CJK JP"
    property bool closing: false
    property bool usageReady: false
    function recordUsage() {
        if (usageReady && !closing)
            player.record_ui_state(showGuide, showChannels, danmaku.item ? danmaku.item.activeCount : 0);
    }
    onShowGuideChanged: recordUsage()
    onShowChannelsChanged: recordUsage()
    Timer { interval: 10000; repeat: true; running: root.usageReady && !root.closing; onTriggered: root.recordUsage() }
    Connections {
        target: player
        function onPlayingChanged() { root.recordUsage(); }
        function onSubtitles_enabledChanged() { root.recordUsage(); }
        function onDanmaku_enabledChanged() { root.recordUsage(); }
        function onComments_enabledChanged() { root.recordUsage(); }
        function onEpg_enabledChanged() { root.recordUsage(); }
    }
    property bool showGuide: false
    property bool showChannels: false
    property bool showStats: false
    property bool showProgram: false
    property int sidebarPage: ProgramSidebar.Program
    readonly property bool summariesVisible: !closing && (channelPanel.active || (sidebar.active && sidebarPage === ProgramSidebar.Channels))
    readonly property bool commentaryVisible: !closing && sidebar.active && sidebarPage === ProgramSidebar.Comments
    onCommentaryVisibleChanged: player.comments_open(commentaryVisible)
    onSummariesVisibleChanged: player.browser_open(summariesVisible)
    readonly property real panelWidth: Math.min(408, Math.max(360, width * 0.32))
    readonly property var channelRows: JSON.parse(player.channel_data)
    Player {
        id: player
    }
    function step(offset) {
        overlayVisibility.reveal();
        player.step_channel(offset);
    }
    function toggleGuide() {
        overlayVisibility.reveal();
        if (!player.epg_enabled)
            return;
        player.guide_open(!root.showGuide);
        root.showGuide = !root.showGuide;
    }
    function closeTopmost() {
        overlayVisibility.reveal();
        // The guide covers the channel picker; close the visible layer first.
        if (root.showGuide)
            root.toggleGuide();
        else if (root.showChannels)
            root.showChannels = false;
        else if (root.showStats)
            root.showStats = false;
        else if (root.showProgram)
            root.showProgram = false;
        else
            windowActions.leaveFullscreen();
    }
    OverlayVisibility {
        id: overlayVisibility
        enabled: !root.closing
        playing: player.playing
        // Like main, the persistent sidebar does not pin the video controls.
        pinned: root.showChannels || root.showGuide || windowActions.popupOpen || windowActions.editingText || volumeSlider.pressed
    }
    AudioSettings {
        id: audioSettings
        windowWidth: root.width
        windowHeight: root.height
        playing: player.playing
        onRefreshRequested: {
            tracksJson = player.audio_tracks();
            errorText = player.audio_error();
        }
        onSelectRequested: function (trackId) {
            errorText = player.select_audio(trackId);
        }
        onClosed: overlayVisibility.reveal()
    }
    PlaybackSettings {
        id: playbackSettings
        videoWidth: video.width
        windowHeight: root.height
        commentsEnabled: player.comments_enabled
        danmakuEnabled: player.danmaku_enabled
        textSize: player.comment_font_size
        textOpacity: player.comment_opacity
        speed: player.comment_speed
        statsVisible: root.showStats
        onDanmakuRequested: function(enabled, size, opacity, speed) {
            player.configure_danmaku(enabled, size, opacity, speed);
        }
        onStatsRequested: function(visible) { root.showStats = visible; }
        onClosed: {
            if (!root.closing) player.save_settings();
            overlayVisibility.reveal();
        }
    }
    WindowActions {
        id: windowActions
        targetWindow: root
        enabled: !root.closing
        guideEnabled: player.epg_enabled
        onFullscreenChanged: overlayVisibility.reveal()
        onChannelsToggleRequested: {
            overlayVisibility.reveal();
            root.showChannels = !root.showChannels;
        }
        onGuideToggleRequested: root.toggleGuide()
        onChannelStepRequested: function (offset) {
            root.step(offset);
        }
        onEscapeRequested: root.closeTopmost()
    }
    Timer {
        interval: 50
        repeat: true
        running: !root.closing
        onTriggered: player.poll()
    }
    Timer {
        interval: 16
        repeat: true
        running: !root.closing && player.subtitles_active
        onTriggered: player.poll_subtitles()
    }
    onClosing: function(close) {
        root.closing = true;
        if (!player.shutdown()) {
            close.accepted = false;
            root.closing = false;
        }
    }
    Component.onCompleted: {
        root.usageReady = true;
        root.recordUsage();
        surface.forceActiveFocus();
        if (player.attach(video) && player.server.length)
            player.connect_server(player.server);
        if (!player.server.length)
            settings.open();
    }
    Item {
        id: surface
        width: root.width - (root.showProgram ? root.panelWidth : 0)
        height: root.height
        focus: true
        signal activity
        onActivity: overlayVisibility.reveal()
        Component.onCompleted: player.observe_pointer(surface)
        GstGLQt6VideoItem {
            id: video
            width: parent.width
            height: root.showProgram ? Math.min(parent.height, width * 9 / 16) : parent.height
            anchors.verticalCenter: parent.verticalCenter
        }
        MouseArea {
            // Below the panels: only a click on the video leaves text editing.
            anchors.fill: parent
            onClicked: {
                surface.forceActiveFocus();
                overlayVisibility.reveal();
            }
        }
        HoverHandler {
            cursorShape: overlayVisibility.controlsVisible ? Qt.ArrowCursor : Qt.BlankCursor
        }
        Loader {
            anchors.fill: parent
            active: !root.closing && !player.playing
            sourceComponent: StoppedPlayback {
                status: player.status
                playbackError: player.playback_error
                playbackMessage: player.playback_message
                canPlay: player.selected >= 0
                hasChannels: root.channelRows.length > 0
                loading: player.loading
                onPlayRequested: player.play()
                onChannelsRequested: {
                    if (player.playback_error.length) player.refresh_channels(true)
                    root.showChannels = true
                }
                onSettingsRequested: settings.open()
            }
        }
        WindowDragArea {
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
            }
            height: 76
            targetWindow: root
            enabled: !root.closing && !root.showGuide && !root.showChannels
            onActivity: overlayVisibility.reveal()
        }
        Loader {
            id: danmaku
            anchors.centerIn: video
            width: video.width
            height: Math.min(video.height, width * 9 / 16)
            active: !root.closing && player.comments_enabled && player.danmaku_enabled && player.playing
            sourceComponent: DanmakuOverlay {
                fontSize: player.comment_font_size
                textOpacity: player.comment_opacity
                speed: player.comment_speed
                fullScreen: root.visibility === Window.FullScreen
                titleOverlapsVideo: programIdentity.visible
                    && programIdentity.y < danmaku.y + danmaku.height
                    && programIdentity.y + programIdentity.height > danmaku.y
                titleBottomInVideo: programIdentity.y + programIdentity.height - danmaku.y
                controlsOverlapVideo: bottomPanel.visible
                    && bottomPanel.y < danmaku.y + danmaku.height
                    && bottomPanel.y + bottomPanel.height > danmaku.y
                controlsTopInVideo: bottomPanel.y - danmaku.y
            }
        }
        Connections {
            target: player
            function onSelectedChanged() { if (danmaku.item) danmaku.item.clearComments(); }
            function onServerChanged() { if (danmaku.item) danmaku.item.clearComments(); }
            function onCommentReceived(text, position, color) {
                if (danmaku.item)
                    danmaku.item.receive(text, position, color);
            }
        }
        Loader {
            // Match a 16:9 broadcast's letterboxed video area.
            anchors.centerIn: parent
            width: Math.min(parent.width, parent.height * 16 / 9)
            height: width * 9 / 16
            active: !root.closing && player.subtitles_active && player.subtitle_display
            sourceComponent: Component {
                SubtitleOverlay {
                    captionJson: player.subtitle_data
                    outlineProvider: player
                }
            }
        }
        Rectangle {
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
            }
            height: Math.min(210, parent.height * 0.28)
            visible: overlayVisibility.controlsVisible
            gradient: Gradient {
                GradientStop {
                    position: 0
                    color: "#a8000000"
                }
                GradientStop {
                    position: 1
                    color: "#00000000"
                }
            }
        }
        Loader {
            id: programIdentity
            anchors {
                left: parent.left
                top: parent.top
                margins: 24
            }
            width: Math.max(360, surface.width - 430)
            active: !root.closing
            visible: overlayVisibility.controlsVisible && !root.showGuide
            sourceComponent: CurrentProgram {
                programJson: player.current_program_data
                onDetailsRequested: {
                    root.sidebarPage = ProgramSidebar.Program;
                    root.showProgram = true;
                }
                channelLabel: player.selected >= 0 && player.selected < root.channelRows.length ? root.channelRows[player.selected].label : ""
                logoUrl: player.selected >= 0 && player.selected < root.channelRows.length ? root.channelRows[player.selected].logo : ""
            }
        }
        Row {
            anchors {
                right: parent.right
                top: parent.top
                margins: 18
            }
            spacing: 14
            z: 7
            visible: overlayVisibility.controlsVisible && !root.showProgram
            IconAction {
                iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/calendar-days.svg"
                tip: qsTranslate("Main", "Program guide")
                enabled: player.epg_enabled
                onClicked: root.toggleGuide()
            }
            IconAction {
                iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/settings-2.svg"
                tip: qsTranslate("Main", "Settings")
                onClicked: settings.open()
            }
            WindowButtons {
                targetWindow: root
            }
        }
        SettingsDrawer {
            id: settings
            onClosed: if (!root.closing) player.save_settings()
            backend: player
            statsVisible: root.showStats
            onStatsRequested: function (visible) {
                root.showStats = visible;
            }
            onEpgDisabled: root.showGuide = false
            onConnectionAccepted: {
                player.guide_open(false);
                root.showGuide = false;
                overlayVisibility.reveal();
            }
        }
        Rectangle {
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            height: Math.min(360, parent.height * 0.46)
            opacity: bottomPanel.opacity
            visible: opacity > 0
            gradient: Gradient {
                GradientStop {
                    position: 0
                    color: "#00000000"
                }
                GradientStop {
                    position: 1
                    color: "#d6000000"
                }
            }
        }
        Pane {
            id: bottomPanel
            anchors {
                bottom: parent.bottom
                left: parent.left
                right: parent.right
            }
            padding: 0
            bottomPadding: 22
            visible: opacity > 0
            enabled: overlayVisibility.controlsVisible && !root.showChannels
            opacity: enabled ? 1 : 0
            Behavior on opacity {
                NumberAnimation {
                    duration: 130
                    easing.type: Easing.OutCubic
                }
            }
            transform: Translate {
                y: root.showChannels ? 20 : 0
                Behavior on y {
                    NumberAnimation {
                        duration: 180
                        easing.type: Easing.OutCubic
                    }
                }
            }
            background: Rectangle {
                color: "transparent"
            }
            contentItem: ColumnLayout {
                spacing: 12
                Label {
                    text: Math.round(player.program_progress * 100) + "%"
                    color: "#b6bab6"
                    font.pixelSize: 11
                    Layout.leftMargin: 24
                }
                ProgressBar {
                    Layout.fillWidth: true
                    Layout.leftMargin: 24
                    Layout.rightMargin: 24
                    Layout.preferredHeight: 4
                    from: 0
                    to: 1
                    value: player.program_progress
                    background: Rectangle {
                        color: "#42ffffff"
                        radius: 2
                    }
                    contentItem: Item {
                        Rectangle {
                            width: parent.width * player.program_progress
                            height: 3
                            radius: 2
                            color: "#e1e1df"
                        }
                    }
                }

                RowLayout {
                    Layout.fillWidth: true
                    Layout.leftMargin: 24
                    Layout.rightMargin: 24
                    spacing: 12
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/" + (player.playing ? "square.svg" : "play.svg")
                        tip: player.playing ? qsTranslate("Main", "Stop") : qsTranslate("Viewer", "Play")
                        primary: !player.playing
                        enabled: player.playing || player.selected >= 0
                        onClicked: player.playing ? player.stop() : player.play()
                    }
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/" + (player.audio_muted || player.volume_level === 0 ? "volume-x.svg" : "volume-2.svg")
                        tip: player.audio_muted ? qsTranslate("Viewer", "Unmute") : qsTranslate("Viewer", "Mute")
                        active: player.audio_muted
                        onClicked: player.mute(!player.audio_muted)
                    }
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/chevron-down.svg"
                        tip: qsTranslate("Viewer", "Audio selection")
                        implicitWidth: 28
                        implicitHeight: 28
                        onClicked: audioSettings.open()
                    }
                    VolumeSlider {
                        id: volumeSlider
                        value: player.volume_level
                        subdued: player.audio_muted
                        closing: root.closing
                        Layout.preferredWidth: 132
                        onVolumeRequested: function (fraction) { player.volume(fraction) }
                        onSaveRequested: player.save_settings()
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/grid-2x2.svg"
                        tip: qsTranslate("Main", "Channels")
                        onClicked: root.showChannels = true
                    }
                    // Presentation-only in main as well; posting is not implemented there.
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/pencil.svg"
                        tip: qsTranslate("Main", "Post a comment")
                    }
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/captions.svg"
                        tip: player.subtitle_display ? qsTranslate("Main", "Hide subtitles") : qsTranslate("Main", "Show subtitles")
                        active: player.subtitles_enabled && player.subtitle_display
                        enabled: player.subtitles_enabled
                        onClicked: player.display_subtitles(!player.subtitle_display)
                    }
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/settings-2.svg"
                        tip: qsTranslate("Main", "Playback settings")
                        onClicked: { playbackSettings.open(); overlayVisibility.reveal(); }
                    }
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/maximize.svg"
                        tip: qsTranslate("Main", "Fullscreen")
                        onClicked: windowActions.toggleFullscreen()
                    }
                    Rectangle {
                        width: 1
                        height: 28
                        color: "#28ffffff"
                        Layout.leftMargin: 4
                        Layout.rightMargin: 4
                    }
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/" + (root.showProgram ? "panel-right-close.svg" : "panel-right-open.svg")
                        tip: root.showProgram ? qsTranslate("Main", "Close side panel") : qsTranslate("Main", "Program information")
                        onClicked: root.showProgram = !root.showProgram
                    }
                }
            }
        }
        Loader {
            anchors.fill: parent
            z: 500
            active: !root.closing && player.epg_enabled && root.showGuide
            visible: active
            sourceComponent: Component {
                ProgramGuide {
                    id: guidePanel
                    uiLanguage: player.ui_language
                    onWatchRequested: function(key) {
                        const error = player.watch_program(key)
                        if (error.length) guidePanel.watchError = error
                        else root.showGuide = false
                    }
                    targetWindow: root
                    onSettingsRequested: settings.open()
                    rows: root.channelRows
                    visibilityJson: player.guide_visibility_data
                    programsJson: player.epg_data
                    status: player.epg_status
                    channel: player.selected >= 0 && player.selected < root.channelRows.length ? root.channelRows[player.selected].label : ""
                    onDayRequested: function (start, end) {
                        player.guide_day(start, end);
                    }
                    onCloseRequested: {
                        root.showGuide = false;
                        player.guide_open(false);
                    }
                }
            }
        }
        MouseArea {
            anchors {
                left: parent.left
                right: parent.right
                top: parent.top
                bottom: channelPanel.top
            }
            visible: root.showChannels && !root.closing
            z: 5
            cursorShape: Qt.PointingHandCursor
            onClicked: root.showChannels = false
        }
        AnimatedPanel {
            id: channelPanel
            anchors {
                left: parent.left
                right: parent.right
                bottom: parent.bottom
            }
            height: Math.min(304, surface.height - 150)
            open: root.showChannels
            shuttingDown: root.closing
            z: 6
            onLoaded: item.openBrowser()
            onOpenChanged: {
                if (open && item)
                    item.openBrowser();
            }
            sourceComponent: ChannelBrowser {
                rows: root.channelRows
                activityJson: player.activity_data
                programsJson: player.channel_program_data
                visibilityJson: player.channel_visibility_data
                now: player.channel_program_now
                selected: player.selected
                onSelectRequested: function (index) {
                    player.select(index);
                    root.showChannels = false;
                }
                onCloseRequested: root.showChannels = false
            }
        }
        Loader {
            x: 16
            // Reserve the playback controls' measured space at small window sizes.
            // Loader.height follows VideoStats' content height, including wrapping.
            y: Math.min(overlayVisibility.controlsVisible ? 138 : 20,
                        Math.max(16, (overlayVisibility.controlsVisible ? bottomPanel.y : surface.height) - height - 16))
            width: Math.min(510, parent.width - 32)
            active: !root.closing && root.showStats && !root.showGuide
            z: 4
            sourceComponent: Component {
                VideoStats {
                    backend: player
                    viewportSize: Qt.size(video.width, video.height)
                    viewportDpr: video.Screen.devicePixelRatio
                    onCloseRequested: root.showStats = false
                }
            }
        }
    }
    SidePanel {
        id: sidebar
        width: root.panelWidth
        open: root.showProgram
        shuttingDown: root.closing
        sourceComponent: ProgramSidebar {
            danmakuEnabled: player.danmaku_enabled
            commentsEnabled: player.comments_enabled
            onDanmakuRequested: function (enabled) {
                player.configure_danmaku(enabled, player.comment_font_size, player.comment_opacity, player.comment_speed);
                player.save_settings();
            }
            commentsJson: player.comment_data
            commentStatus: player.comment_status
            commentProgramTitle: player.comment_program_title
            page: root.sidebarPage
            channelRows: root.channelRows
            selectedChannel: player.selected
            activityJson: player.activity_data
            channelPrograms: player.channel_program_data
            channelVisibility: player.channel_visibility_data
            now: player.channel_program_now
            onPageRequested: function (page) {
                root.sidebarPage = page;
            }
            onSelectRequested: function (index) {
                player.select(index);
            }
            targetWindow: root
            programJson: player.current_program_data
            progress: player.program_progress
            channelLabel: player.selected >= 0 && player.selected < root.channelRows.length ? root.channelRows[player.selected].label : ""
            logoUrl: player.selected >= 0 && player.selected < root.channelRows.length ? root.channelRows[player.selected].logo : ""
            guideEnabled: player.epg_enabled
            onCloseRequested: root.showProgram = false
            onGuideRequested: root.toggleGuide()
            onSettingsRequested: settings.open()
        }
    }
    WindowResizeFrame {
        anchors.fill: parent
        z: 1000
        targetWindow: root
        enabled: !root.closing
    }
}
