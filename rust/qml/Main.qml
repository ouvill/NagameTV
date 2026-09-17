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
    title: player.recording ? player.recording_name : programIdentity.item && programIdentity.item.program && programIdentity.item.program.name
        ? programIdentity.item.program.name : qsTranslate("Main", "Mirakurun Viewer")
    color: "#0b0c0b"
    font.family: "Noto Sans CJK JP"
    property bool closing: false
    readonly property bool setupRequired: !player.server_configured
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
        function onDanmaku_enabledChanged() { root.recordUsage(); }
        function onComments_enabledChanged() { root.recordUsage(); }
    }
    readonly property bool showGuide: player.guide_visible
    property bool showChannels: false
    property bool showStats: false
    property bool showProgram: false
    property bool showCommentComposer: false
    function closeCommentComposer() {
        root.showCommentComposer = false;
        surface.forceActiveFocus();
        overlayVisibility.reveal();
    }
    property int sidebarPage: ProgramSidebar.Program
    readonly property bool summariesVisible: !closing && (channelPanel.active || (sidebar.active && sidebarPage === ProgramSidebar.Channels))
    readonly property bool commentaryVisible: !closing && sidebar.active && sidebarPage === ProgramSidebar.Comments
    readonly property bool guideVisible: !closing && player.epg_enabled && showGuide
    onCommentaryVisibleChanged: player.comments_open(commentaryVisible)
    onSummariesVisibleChanged: player.browser_open(summariesVisible)
    readonly property real panelWidth: Math.min(408, Math.max(360, width * 0.32))
    readonly property var channelRows: JSON.parse(player.channel_data)
    Player {
        id: player
    }
    RecordingSeekSteps { id: recordingSeekSteps }
    RecordingInput {
        id: recordingInput
        anchors.fill: parent
        z: 100
        backend: player
        enabled: !root.closing
        onStarted: {
            setup.close();
            player.guide_open(false);
            root.showChannels = false;
            root.showProgram = false;
            root.closeCommentComposer();
        }
    }
    Shortcut {
        sequence: "Ctrl+O"
        context: Qt.WindowShortcut
        autoRepeat: false
        enabled: !root.closing
        onActivated: recordingInput.open()
    }
    function openConnectionSettings() {
        if (root.setupRequired) setup.open();
        else {
            settings.page = SettingsPanel.Connection;
            settings.open();
        }
    }
    Shortcut {
        sequence: "Space"
        context: Qt.WindowShortcut
        autoRepeat: false
        enabled: windowActions.navigationEnabled && (player.recording || player.timeshift) && !root.showGuide && !root.showChannels
        onActivated: {
            player.playing ? player.pause() : player.play();
            overlayVisibility.reveal();
        }
    }
    Shortcut {
        sequence: "Left"
        context: Qt.WindowShortcut
        enabled: windowActions.navigationEnabled && player.seekable && !root.showGuide && !root.showChannels
            && !(root.activeFocusItem instanceof Slider)
        onActivated: { player.skip(recordingSeekSteps.backwardMilliseconds); overlayVisibility.reveal(); }
    }
    Shortcut {
        sequence: "Right"
        context: Qt.WindowShortcut
        enabled: windowActions.navigationEnabled && player.seekable && !root.showGuide && !root.showChannels
            && !(root.activeFocusItem instanceof Slider)
        onActivated: { player.skip(recordingSeekSteps.forwardMilliseconds); overlayVisibility.reveal(); }
    }
    function chooseConnectedChannel() {
        player.guide_open(false);
        root.showChannels = true;
        overlayVisibility.reveal();
    }
    function requestMode(mode) {
        overlayVisibility.reveal();
        switch (mode) {
        case ModeNavigation.Live:
            player.cancel_recording_open();
            if (root.setupRequired) {
                setup.open();
            } else if (player.recording && player.selected >= 0) {
                player.select(player.selected);
            } else if (player.recording || root.channelRows.length === 0) {
                root.chooseConnectedChannel();
            }
            break;
        case ModeNavigation.Recording:
            // Replace this destination with the recording library when available.
            // Selection/cancellation leaves the current playback mode unchanged.
            recordingInput.open();
            break;
        case ModeNavigation.Guide:
            root.toggleGuide();
            break;
        case ModeNavigation.Settings:
            settings.open();
            break;
        }
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
    }
    function closeTopmost() {
        overlayVisibility.reveal();
        // The guide covers the channel picker; close the visible layer first.
        if (root.showGuide)
            root.toggleGuide();
        else if (root.showChannels)
            root.showChannels = false;
        else if (root.showCommentComposer)
            root.closeCommentComposer();
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
        pinned: root.showChannels || root.showGuide || windowActions.popupOpen || windowActions.editingText || playerControls.volumePressed || recordingTimeline.pressed || recordingTimeline.hovered
    }
    AudioSettings {
        id: audioSettings
        windowWidth: root.width
        windowHeight: root.height
        playing: player.media_active
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
        timeshiftStorage: player.timeshift_storage
        onTimeshiftRequested: function(storage) { player.configure_timeshift(storage); }
        onTimeshiftSettingsRequested: { settings.open(); settings.page = SettingsPanel.Timeshift; }
        toggleButton: playerControls.settingsButton
        commentsEnabled: player.comments_enabled
        danmakuEnabled: player.danmaku_enabled
        textSize: player.comment_font_size
        textOpacity: player.comment_opacity
        speed: player.comment_speed
        shadowEnabled: player.comment_shadow_enabled
        statsVisible: root.showStats
        onDanmakuRequested: function(enabled, size, opacity, speed) {
            player.configure_danmaku(enabled, size, opacity, speed);
        }
        onStatsRequested: function(visible) { root.showStats = visible; }
        onShadowRequested: function(enabled) { player.configure_comment_shadow(enabled); }
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
        screenshotEnabled: screenshot.canCapture && !root.showGuide && !root.showChannels
        onScreenshotRequested: screenshot.capture()
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
    ScreenshotCapture {
        id: screenshot
        backend: player
        target: videoPicture
        available: player.media_active && !player.seeking
        enabled: !root.closing
        onSaved: function(file) { screenshotNotice.showSaved(file); }
        onFailed: function(message) { screenshotNotice.showFailure(message); }
    }
    Connections {
        target: root
        function onAfterAnimating() {
            if (root.closing || !player.media_active || video.width <= 0 || video.height <= 0) return;
            const layers = [];
            if (danmaku.active && danmaku.item && danmaku.visible && danmaku.item.visible)
                layers.push(danmaku.item.screenshotLayer(video));
            if (captions.active && captions.item && captions.visible && captions.item.visible)
                layers.push(captions.item.screenshotLayer(video));
            player.stage_screenshot(JSON.stringify({width: video.width, height: video.height, layers: layers}));
        }
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
        if (player.attach(video) && !root.setupRequired)
            player.connect_server(player.server);
        if (root.setupRequired)
            setup.open();
    }
    Item {
        id: surface
        width: root.width - (root.showProgram ? root.panelWidth : 0)
        height: root.height
        focus: true
        signal activity
        onActivity: overlayVisibility.reveal()
        Component.onCompleted: player.observe_pointer(surface)
        Rectangle {
            id: videoPicture
            color: "#0b0c0b"
            width: parent.width
            height: root.showProgram ? Math.min(parent.height, width * 9 / 16) : parent.height
            anchors.verticalCenter: parent.verticalCenter
            GstGLQt6VideoItem {
                id: video
                anchors.fill: parent
            }
            Loader {
                id: danmaku
                anchors.centerIn: video
                width: video.width
                height: Math.min(video.height, width * 9 / 16)
                active: !root.closing && player.comments_enabled && player.danmaku_enabled && player.media_active
                sourceComponent: DanmakuOverlay {
                    id: playbackComments
                    playbackClock: player
                    paused: player.paused || player.seeking || player.ended
                    property string timelineJson: player.comment_timeline
                    property var replayGeneration: null
                    property bool replayReady: false
                    function syncTimeline() {
                        if (!replayReady) return;
                        const snapshot = JSON.parse(timelineJson);
                        const position = player.commentary_position();
                        if (!snapshot || position < 0) return;
                        const reset = replayGeneration !== snapshot.generation;
                        if (controller.update_timeline(JSON.stringify(snapshot.comments), position, reset))
                            replayGeneration = snapshot.generation;
                    }
                    onTimelineJsonChanged: syncTimeline()
                    Component.onCompleted: { configure(); replayReady = true; syncTimeline(); }
                    fontSize: player.comment_font_size
                    textOpacity: player.comment_opacity
                    speed: player.comment_speed
                    shadowEnabled: player.comment_shadow_enabled
                    fullScreen: root.visibility === Window.FullScreen
                    titleOverlapsVideo: programIdentity.visible
                        && programIdentity.y < videoPicture.y + danmaku.y + danmaku.height
                        && programIdentity.y + programIdentity.height > videoPicture.y + danmaku.y
                    titleBottomInVideo: programIdentity.y + programIdentity.height - videoPicture.y - danmaku.y
                    controlsOverlapVideo: (bottomPanel.visible || composer.visible)
                        && controlsTopInVideo < danmaku.height
                        && surface.height > videoPicture.y + danmaku.y
                    controlsTopInVideo: (composer.visible
                        ? composer.y + composer.height - composer.occupiedHeight : bottomPanel.y) - videoPicture.y - danmaku.y
                }
            }
            Loader {
                // Match a 16:9 broadcast's letterboxed video area.
                id: captions
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
        }
        MouseArea {
            // Below the panels: only a click on the video leaves text editing.
            anchors.fill: parent
            onClicked: {
                root.showCommentComposer = false;
                surface.forceActiveFocus();
                overlayVisibility.reveal();
            }
        }
        HoverHandler {
            cursorShape: overlayVisibility.controlsVisible ? Qt.ArrowCursor : Qt.BlankCursor
        }
        Loader {
            anchors.fill: parent
            active: !root.closing && !player.media_active
            sourceComponent: StoppedPlayback {
                status: player.status
                playbackError: player.playback_error
                playbackMessage: player.playback_message
                canPlay: player.recording || player.selected >= 0
                recording: player.recording
                hasChannels: root.channelRows.length > 0
                hasServer: player.server_configured
                loading: player.loading || player.connecting
                onPlayRequested: player.play()
                onChannelsRequested: {
                    if (player.playback_error.length) player.refresh_channels(true)
                    root.showChannels = true
                }
                onSettingsRequested: root.openConnectionSettings()
                onReconnectRequested: player.connect_server(player.server)
                onOpenFileRequested: recordingInput.open()
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
            active: !root.closing && (!player.recording || player.current_program_data !== "null")
            visible: overlayVisibility.controlsVisible && !root.showGuide
            sourceComponent: CurrentProgram {
                recording: player.recording
                fallbackTitle: player.recording_name
                programJson: player.current_program_data
                programStatus: player.program_status
                onDetailsRequested: {
                    root.sidebarPage = ProgramSidebar.Program;
                    root.showProgram = true;
                }
                channelLabel: player.recording ? (program && program.station || player.recording_name) : player.selected >= 0 && player.selected < root.channelRows.length ? root.channelRows[player.selected].label : ""
                logoUrl: !player.recording && player.selected >= 0 && player.selected < root.channelRows.length ? root.channelRows[player.selected].logo : ""
            }
        }
        Label {
            anchors { left: parent.left; top: parent.top; margins: 24 }
            width: Math.max(220, surface.width - 580)
            visible: player.recording && player.current_program_data === "null" && overlayVisibility.controlsVisible && !root.showGuide
            text: player.recording_name
            textFormat: Text.PlainText
            color: "#f4f5f3"
            font.pixelSize: 23
            font.bold: true
            wrapMode: Text.Wrap
            maximumLineCount: 2
            elide: Text.ElideRight
        }
        SettingsPanel {
            id: settings
            targetWindow: root
            onClosed: if (!root.closing) player.save_settings()
            backend: player
            statsVisible: root.showStats
            onStatsRequested: function (visible) {
                root.showStats = visible;
            }
            onConnectionAccepted: root.chooseConnectedChannel()
        }
        FirstRunSetup {
            id: setup
            backend: player
            targetWindow: root
            onCompleted: root.chooseConnectedChannel()
            onOpenFileRequested: recordingInput.open()
            onFileDropped: function(file) { recordingInput.openUrl(file); }
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
            enabled: overlayVisibility.controlsVisible && !root.showChannels && !root.showCommentComposer
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
                PlaybackTimeline {
                    id: recordingTimeline
                    Layout.fillWidth: true
                    Layout.leftMargin: 24
                    Layout.rightMargin: 24
                    backend: player
                    closing: root.closing
                }
                PlayerControls {
                    id: playerControls
                    Layout.fillWidth: true
                    Layout.leftMargin: 24
                    Layout.rightMargin: 24
                    backend: player
                    closing: root.closing
                    canCapture: screenshot.canCapture
                    settingsVisible: playbackSettings.visible
                    sidePanelOpen: root.showProgram
                    onAudioRequested: audioSettings.open()
                    onChannelsRequested: root.showChannels = true
                    onCommentRequested: {
                        root.showCommentComposer = true;
                        composer.focusEditor();
                    }
                    onCaptureRequested: screenshot.capture()
                    onSettingsRequested: { playbackSettings.toggle(); overlayVisibility.reveal(); }
                    onFullscreenRequested: windowActions.toggleFullscreen()
                    onSidePanelRequested: root.showProgram = !root.showProgram
                }
            }
        }
        ScreenshotNotice {
            id: screenshotNotice
            objectName: "screenshotNotice"
            anchors { horizontalCenter: parent.horizontalCenter; top: parent.top; topMargin: 90 }
            width: Math.min(560, parent.width - 48)
            z: 8
            onOpenFolderRequested: function(file) {
                if (player.open_screenshot_file_directory(file)) dismiss();
                else showFailure(player.screenshot_error);
            }
        }
        CommentComposer {
            id: composer
            anchors { horizontalCenter: parent.horizontalCenter; bottom: parent.bottom; bottomMargin: 16 }
            width: Math.min(760, parent.width - 48)
            visible: root.showCommentComposer && !root.showChannels && !root.showGuide && !root.closing
            draft: player.comment_draft
            status: player.comment_post_status
            available: player.comment_post_available
            supported: player.comments_enabled && player.comment_post_target.length > 0
            busy: player.comment_post_busy
            sendOnEnter: player.comment_send_on_enter
            onDraftEdited: function(text) { player.edit_comment_draft(text); }
            onSendRequested: player.post_comment()
        }
        Loader {
            anchors.fill: parent
            z: 500
            active: root.guideVisible
            visible: active
            sourceComponent: Component {
                ProgramGuide {
                    id: guidePanel
                    uiLanguage: player.ui_language
                    onWatchRequested: function(key) {
                        const error = player.watch_program(key)
                        if (error.length) guidePanel.watchError = error
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
                    onCloseRequested: player.guide_open(false)
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
            commentModel: player.comment_model
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
            programStatus: player.program_status
            progress: player.program_progress
            recording: player.recording
            fallbackTitle: player.recording_name
            channelLabel: player.recording ? (program && program.station || player.recording_name) : player.selected >= 0 && player.selected < root.channelRows.length ? root.channelRows[player.selected].label : ""
            logoUrl: !player.recording && player.selected >= 0 && player.selected < root.channelRows.length ? root.channelRows[player.selected].logo : ""
            onCloseRequested: root.showProgram = false
        }
    }
    ModeNavigation {
        id: modeNavigation
        anchors { right: parent.right; top: parent.top; margins: 18 }
        z: 20
        targetWindow: root
        enabled: !root.closing
        visible: !root.showGuide && !settings.visible && (root.showProgram || overlayVisibility.controlsVisible)
        mode: player.recording ? ModeNavigation.Recording : ModeNavigation.Live
        guideEnabled: player.epg_enabled
        onModeRequested: function(mode) { root.requestMode(mode); }
    }
    WindowResizeFrame {
        anchors.fill: parent
        z: 1000
        targetWindow: root
        enabled: !root.closing
    }
}
