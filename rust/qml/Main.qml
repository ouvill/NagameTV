import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.freedesktop.gstreamer.Qt6GLVideoItem 1.0
import MinimalViewer 1.0

ViewerWindow {
    id: root
    flags: Qt.Window | Qt.FramelessWindowHint
    title: player.recording ? player.recording_name : programIdentity.item && programIdentity.item.program && programIdentity.item.program.name
        ? programIdentity.item.program.name : qsTranslate("Main", "NagameTV")
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
        function onDesktop_raise_requested() {
            if (root.visibility === Window.Minimized) root.showNormal();
            root.raise();
            root.requestActivate();
        }
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
    property int sidebarPage: ProgramSidebar.Playback
    readonly property bool summariesVisible: !closing && (channelPanel.active || (sidebar.active && sidebarPage === ProgramSidebar.Channels))
    readonly property bool commentaryVisible: !closing && sidebar.active && sidebarPage === ProgramSidebar.Program
    readonly property bool guideVisible: !closing && player.epg_enabled && showGuide
    onCommentaryVisibleChanged: player.comments_open(commentaryVisible)
    onSummariesVisibleChanged: player.browser_open(summariesVisible)
    readonly property real panelWidth: Math.min(408, Math.max(320, viewport.width * 0.32))
    readonly property var channelRows: JSON.parse(player.channel_data)
    Player {
        id: player
    }
    RecordingInput {
        id: recordingInput
        anchors.fill: parent
        z: 100
        backend: player
        enabled: !root.closing
        onStarted: {
            setup.close();
            settings.close();
            player.guide_open(false);
            root.showChannels = false;
            root.showProgram = false;
            root.closeCommentComposer();
        }
    }
    function openConnectionSettings() {
        if (root.setupRequired) setup.open();
        else {
            settings.page = SettingsPanel.Connection;
            settings.open();
        }
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
            settings.close();
            player.cancel_recording_open();
            player.guide_open(false);
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
            viewerActions.openRecording.trigger();
            break;
        case ModeNavigation.Guide:
            if (!player.epg_enabled) break;
            if (settings.visible) player.guide_open(true);
            else viewerActions.toggleGuide.trigger();
            settings.close();
            break;
        case ModeNavigation.Settings:
            settings.open();
            break;
        }
    }
    OverlayVisibility {
        id: overlayVisibility
        enabled: !root.closing
        playing: player.playing
        // Like main, the persistent sidebar does not pin the video controls.
        pinned: root.showChannels || root.showGuide || inputContext.popupOpen || inputContext.editingText || playerControls.screenshotHovered || recordingTimeline.pressed || recordingTimeline.hovered
    }
    AudioSettings {
        id: audioSettings
        shuttingDown: root.closing
        objectName: "audioSettingsPopup"
        anchorItem: playerControls.audioAnchor
        windowWidth: root.viewport.width
        windowHeight: root.viewport.height
        playing: player.media_active
        volumeLevel: player.volume_level
        muted: player.audio_muted
        onVolumeRequested: function(value) { player.volume(value); }
        onMuteRequested: function(value) { player.mute(value); }
        onSaveRequested: player.save_settings()
        onRefreshRequested: {
            tracksJson = player.audio_tracks();
            errorText = player.audio_error();
        }
        onSelectRequested: function (trackId) {
            errorText = player.select_audio(trackId);
        }
        // Popup restores keyboard focus; do not steal a new outside target's
        // focus when the closing animation finishes.
        onClosed: overlayVisibility.reveal()
    }
    ViewerActions {
        id: viewerActions
        backend: player
        audioVisible: audioSettings.visible
        targetWindow: root
        enabled: !root.closing
        channelsVisible: root.showChannels
        composerVisible: root.showCommentComposer
        statsVisible: root.showStats
        programVisible: root.showProgram
        canCapture: screenshot.canCapture
        onActivity: overlayVisibility.reveal()
        onChannelsVisibilityRequested: function(visible) { root.showChannels = visible; }
        onComposerVisibilityRequested: function(visible) {
            if (visible) { root.showCommentComposer = true; composer.focusEditor(); }
            else root.closeCommentComposer();
        }
        onStatsVisibilityRequested: function(visible) { root.showStats = visible; }
        onProgramVisibilityRequested: function(visible) { root.showProgram = visible; }
        onRecordingRequested: recordingInput.open()
        onCaptureRequested: screenshot.capture()
        onAudioRequested: { playerControls.closeSpeed(); audioSettings.toggle(); }
        onSpeedOpened: audioSettings.close()
        onSettingsRequested: {
            root.showProgram = !(root.showProgram && root.sidebarPage === ProgramSidebar.Playback);
            root.sidebarPage = ProgramSidebar.Playback;
        }
    }
    InputContext {
        id: inputContext
        targetWindow: root
        enabled: !root.closing
        playbackControls: player.recording || player.timeshift
        guideVisible: root.showGuide
        channelsVisible: root.showChannels
    }
    ShortcutBindings {
        id: shortcutBindings
        actions: viewerActions
        inputContext: inputContext
    }
    CommentSubmitPolicy {
        id: commentSubmitPolicy
        mode: player.comment_send_on_enter ? CommentSubmitPolicy.EnterOrControlEnter : CommentSubmitPolicy.ControlEnter
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
        // Native playback shutdown can process pending window events. Preserve
        // the geometry at the close request, before those events change it.
        const closingSize = root.windowSizeToRemember();
        const closingWidth = closingSize.width;
        const closingHeight = closingSize.height;
        root.closing = true;
        if (!player.shutdown()) {
            close.accepted = false;
            root.closing = false;
        } else {
            player.remember_window_size(closingWidth, closingHeight);
        }
    }
    Component.onCompleted: {
        if (!root.initializeWindow(JSON.parse(player.window_options(root.viewport)))) {
            Qt.quit();
            return;
        }
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
        width: root.viewport.width - (sidebar.open ? root.panelWidth : 0)
        height: root.viewport.height
        focus: true
        signal activity
        signal pointerExited
        onActivity: overlayVisibility.pointerActivity()
        onPointerExited: overlayVisibility.pointerExited()
        Component.onCompleted: player.observe_pointer(surface)
        Rectangle {
            id: videoPicture
            color: "#0b0c0b"
            width: parent.width
            height: sidebar.open ? Math.min(parent.height, width * 9 / 16) : parent.height
            anchors.verticalCenter: parent.verticalCenter
            GstGLQt6VideoItem {
                id: video
                anchors.fill: parent
            }
            VideoCommentBounds {
                id: commentBounds
                viewportWidth: video.width
                viewportHeight: video.height
                aspectRatio: player.video_aspect_ratio
                evaluationWide: player.evaluation_wide_comments
            }
            Loader {
                id: danmaku
                x: commentBounds.x
                y: commentBounds.y
                width: commentBounds.width
                height: commentBounds.height
                clip: true
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
                    displayMode: player.comment_display
                    placementMode: player.comment_placement
                    // The saved text size is relative to a 1280 x 720 picture.
                    // This item's height follows the fitted video, excluding bars.
                    readonly property int referenceVideoHeight: 720
                    fontSize: Math.max(1, Math.round(player.comment_font_size * height / referenceVideoHeight))
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
                        forceOutline: player.subtitle_force_outline
                        captionJson: player.subtitle_data
                        outlineProvider: player
                    }
                }
            }
        }
        MouseArea {
            objectName: "videoPointerArea"
            // Panels above this area consume their own clicks/double clicks.
            enabled: !root.closing
            onDoubleClicked: viewerActions.toggleFullscreen.trigger()
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
                onPlayRequested: viewerActions.playbackToggle.trigger()
                onChannelsRequested: {
                    if (player.playback_error.length) player.refresh_channels(true)
                    root.showChannels = true
                }
                onSettingsRequested: root.openConnectionSettings()
                onReconnectRequested: player.connect_server(player.server)
                onOpenFileRequested: viewerActions.openRecording.trigger()
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
            shortcutEntries: shortcutBindings.entries
            commentSubmitPolicy: commentSubmitPolicy
            targetWindow: root
            onClosed: if (!root.closing) player.save_settings()
            backend: player
            statsVisible: root.showStats
            onStatsRequested: function (visible) {
                root.showStats = visible;
            }
            onConnectionAccepted: root.chooseConnectedChannel()
            onModeRequested: function(mode) { root.requestMode(mode); }
        }
        FirstRunSetup {
            id: setup
            backend: player
            targetWindow: root
            onCompleted: root.chooseConnectedChannel()
            onOpenFileRequested: viewerActions.openRecording.trigger()
            onFileDropped: function(file) { recordingInput.openUrl(file); }
            onTransferDropped: function(key) { recordingInput.openTransfer(key); }
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
                spacing: 0
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
                    videoWidth: surface.width
                    actions: viewerActions
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
            submitPolicy: commentSubmitPolicy
            onDraftEdited: function(text) { player.edit_comment_draft(text); }
            onSendRequested: player.post_comment()
        }
        AnimatedPanel {
            id: guideLoader
            parent: root.viewport
            anchors.fill: parent
            z: 500
            motion: AnimatedPanel.Fade
            open: root.guideVisible
            shuttingDown: root.closing
            onLoaded: item.refreshSnapshot()
            onOpenChanged: if (open && item) item.openGuide()
            sourceComponent: Component {
                ProgramGuide {
                    id: guidePanel
                    uiLanguage: player.ui_language
                    onWatchRequested: function(key) {
                        const error = player.watch_program(key)
                        if (error.length) guidePanel.watchError = error
                    }
                    targetWindow: root
                    onModeRequested: function(mode) { root.requestMode(mode); }
                    rows: root.channelRows
                    selected: player.selected
                    viewingIndex: player.viewing_channel
                    programsJson: "[]"
                    status: ""
                    // Closing clears Rust's projection before its visibility
                    // signal. Retain the last frame only for the fade lifetime.
                    function refreshSnapshot() {
                        if (!player.guide_visible) return;
                        visibilityJson = player.guide_visibility_data;
                        programsJson = player.epg_data;
                        status = player.epg_status;
                    }
                    Connections {
                        target: player
                        function onEpg_dataChanged() { guidePanel.refreshSnapshot(); }
                        function onGuide_visibility_dataChanged() { guidePanel.refreshSnapshot(); }
                        function onEpg_statusChanged() { guidePanel.refreshSnapshot(); }
                    }
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
                viewingIndex: player.viewing_channel
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
                    viewportSize: Qt.size(video.width * root.uiScale, video.height * root.uiScale)
                    viewportDpr: video.Screen.devicePixelRatio
                    onCloseRequested: root.showStats = false
                }
            }
        }
    }
    SidePanel {
        id: sidebar
        width: root.panelWidth
        open: root.showProgram && !root.guideVisible
        visible: active && !root.guideVisible
        shuttingDown: root.closing
        onOpenChanged: if (open && item) item.openChannels()
        sourceComponent: ProgramSidebar {
            evaluationCommentList: player.evaluation_comment_list
            evaluationCollision: player.evaluation_collision_layout
            displayMode: player.comment_display
            placementMode: player.comment_placement
            textSize: player.comment_font_size
            textOpacity: player.comment_opacity
            speed: player.comment_speed
            shadowEnabled: player.comment_shadow_enabled
            onPresentationRequested: function(display, placement) { player.configure_comment_presentation(display, placement); }
            onAdjusted: function(size, opacity, speed) {
                player.configure_danmaku(player.danmaku_enabled, size, opacity, speed);
                player.save_settings();
            }
            onShadowRequested: function(enabled) { player.configure_comment_shadow(enabled); }
            danmakuEnabled: player.danmaku_enabled
            commentsEnabled: player.comments_enabled
            onDanmakuRequested: function (enabled) {
                player.configure_danmaku(enabled, player.comment_font_size, player.comment_opacity, player.comment_speed);
                player.save_settings();
            }
            statsVisible: root.showStats
            onStatsRequested: function(visible) { root.showStats = visible; }
            onTimeshiftSettingsRequested: { settings.open(); settings.page = SettingsPanel.Timeshift; }
            commentModel: player.comment_model
            commentStatus: player.comment_status
            commentProgramTitle: player.comment_program_title
            page: root.sidebarPage
            channelRows: root.channelRows
            selectedChannel: player.selected
            viewingIndex: player.viewing_channel
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
        z: 20
        targetWindow: root
        enabled: !root.closing
        visible: !root.showGuide && !settings.visible && (root.showProgram || overlayVisibility.controlsVisible)
        mode: player.recording ? ModeNavigation.Recording : ModeNavigation.Live
        guideEnabled: player.epg_enabled
        onModeRequested: function(mode) { root.requestMode(mode); }
    }
    WindowResizeFrame {
        // Resize handles retain their screen-space hit area at small sizes.
        parent: root.contentItem
        anchors.fill: parent
        z: 1000
        targetWindow: root
        enabled: !root.closing
    }
}
