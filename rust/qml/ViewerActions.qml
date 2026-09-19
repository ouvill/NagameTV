pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import MinimalViewer 1.0

Item {
    id: root
    required property var backend
    required property Window targetWindow
    property bool channelsVisible: false
    property bool composerVisible: false
    property bool statsVisible: false
    property bool programVisible: false
    property bool canCapture: false
    readonly property bool fullscreen: targetWindow !== null && targetWindow.visibility === Window.FullScreen
    property int restoreVisibility: Window.Windowed
    readonly property RecordingSeekSteps seekSteps: RecordingSeekSteps {}
    // Allow the decoder's small receive-to-display delay at the live edge.
    readonly property real liveEdgeToleranceMs: 2500
    readonly property bool atLiveEdge: !backend.recording && backend.media_active && !backend.paused
        && !backend.seeking && (!backend.timeshift || backend.live_delay_ms <= liveEdgeToleranceMs)
    signal activity
    signal channelsVisibilityRequested(bool visible)
    signal composerVisibilityRequested(bool visible)
    signal statsVisibilityRequested(bool visible)
    signal programVisibilityRequested(bool visible)
    signal recordingRequested
    signal captureRequested
    signal audioRequested
    signal settingsRequested

    function leaveFullscreen() {
        if (!fullscreen) return;
        if (restoreVisibility === Window.Maximized) targetWindow.showMaximized();
        else targetWindow.showNormal();
    }
    onFullscreenChanged: activity()

    component Operation: Action {
        enabled: root.enabled
    }
    readonly property Action playbackToggle: Operation {
        text: root.backend.playback_action === Player.Pause ? qsTranslate("Viewer", "Pause")
            : root.backend.playback_action === Player.Stop ? qsTranslate("Main", "Stop") : qsTranslate("Viewer", "Play")
        enabled: root.enabled && root.backend.playback_action !== Player.Unavailable
        onTriggered: { root.backend.toggle_playback(); root.activity(); }
    }
    readonly property string playbackIcon: backend.playback_action === Player.Pause ? "pause.svg"
        : backend.playback_action === Player.Stop ? "square.svg" : "play-outline.svg"
    readonly property Action seekBackward: Operation {
        text: qsTranslate("Viewer", "Back 10 seconds")
        enabled: root.enabled && root.backend.seekable
        onTriggered: { root.backend.skip(root.seekSteps.backwardMilliseconds); root.activity(); }
    }
    readonly property Action seekForward: Operation {
        text: qsTranslate("Viewer", "Forward 30 seconds")
        enabled: root.enabled && root.backend.seekable
        onTriggered: { root.backend.skip(root.seekSteps.forwardMilliseconds); root.activity(); }
    }
    readonly property Action openRecording: Operation {
        text: qsTranslate("Recording", "Open TS file")
        onTriggered: root.recordingRequested()
    }
    readonly property Action returnToLive: Operation {
        text: root.atLiveEdge ? qsTranslate("Viewer", "Live broadcast") : qsTranslate("Viewer", "Return to live")
        enabled: root.enabled && !root.backend.recording && root.backend.media_active
        onTriggered: {
            if (root.backend.timeshift && root.backend.seekable) root.backend.return_to_live();
            root.activity();
        }
    }
    readonly property Action openChannels: Operation {
        text: qsTranslate("Main", "Channels")
        onTriggered: { root.channelsVisibilityRequested(true); root.activity(); }
    }
    readonly property Action toggleChannels: Operation {
        text: qsTranslate("Main", "Channels")
        onTriggered: { root.channelsVisibilityRequested(!root.channelsVisible); root.activity(); }
    }
    readonly property Action toggleGuide: Operation {
        text: qsTranslate("Main", "Program guide")
        enabled: root.enabled && root.backend.epg_enabled
        onTriggered: { root.backend.guide_open(!root.backend.guide_visible); root.activity(); }
    }
    readonly property Action previousChannel: Operation {
        text: qsTranslate("Settings", "Previous channel")
        onTriggered: { root.backend.step_channel(-1); root.activity(); }
    }
    readonly property Action nextChannel: Operation {
        text: qsTranslate("Settings", "Next channel")
        onTriggered: { root.backend.step_channel(1); root.activity(); }
    }
    readonly property Action captureScreenshot: Operation {
        text: qsTranslate("Main", "Save screenshot")
        enabled: root.enabled && root.canCapture
        onTriggered: root.captureRequested()
    }
    readonly property Action toggleFullscreen: Operation {
        text: qsTranslate("Settings", "Toggle fullscreen")
        onTriggered: {
            if (root.fullscreen) root.leaveFullscreen();
            else {
                root.restoreVisibility = root.targetWindow.visibility === Window.Maximized ? Window.Maximized : Window.Windowed;
                root.targetWindow.showFullScreen();
            }
        }
    }
    readonly property Action dismissTopmost: Operation {
        text: qsTranslate("Settings", "Close a panel or leave fullscreen")
        onTriggered: {
            root.activity();
            // Match stacking order: the guide covers the channel browser.
            if (root.backend.guide_visible) root.backend.guide_open(false);
            else if (root.channelsVisible) root.channelsVisibilityRequested(false);
            else if (root.composerVisible) root.composerVisibilityRequested(false);
            else if (root.statsVisible) root.statsVisibilityRequested(false);
            else if (root.programVisible) root.programVisibilityRequested(false);
            else root.leaveFullscreen();
        }
    }
    readonly property Action toggleMute: Operation {
        text: root.backend.audio_muted ? qsTranslate("Viewer", "Unmute") : qsTranslate("Viewer", "Mute")
        onTriggered: root.backend.mute(!root.backend.audio_muted)
    }
    readonly property Action toggleSubtitles: Operation {
        text: root.backend.subtitle_display ? qsTranslate("Main", "Hide subtitles") : qsTranslate("Main", "Show subtitles")
        enabled: root.enabled && root.backend.subtitles_enabled
        onTriggered: root.backend.display_subtitles(!root.backend.subtitle_display)
    }
    readonly property Action toggleDanmaku: Operation {
        text: root.backend.danmaku_enabled ? qsTranslate("Main", "Hide danmaku") : qsTranslate("Main", "Show danmaku")
        enabled: root.enabled && root.backend.comments_enabled && !root.backend.recording
        onTriggered: {
            root.backend.configure_danmaku(!root.backend.danmaku_enabled, root.backend.comment_font_size,
                root.backend.comment_opacity, root.backend.comment_speed);
            root.backend.save_settings();
        }
    }
    readonly property Action openComposer: Operation {
        text: qsTranslate("Main", "Post a comment")
        enabled: root.enabled && root.backend.comments_enabled && !root.backend.recording
        onTriggered: {
            // Reveal the editor before Main gives it focus.
            if (root.backend.guide_visible) root.backend.guide_open(false);
            if (root.channelsVisible) root.channelsVisibilityRequested(false);
            root.composerVisibilityRequested(true);
            root.activity();
        }
    }
    readonly property Action openAudio: Operation {
        text: qsTranslate("Viewer", "Audio selection")
        onTriggered: root.audioRequested()
    }
    readonly property Action toggleSettings: Operation {
        text: qsTranslate("Main", "Playback settings")
        onTriggered: { root.settingsRequested(); root.activity(); }
    }
    readonly property Action toggleProgram: Operation {
        text: root.programVisible ? qsTranslate("Main", "Close side panel") : qsTranslate("Main", "Open side panel")
        onTriggered: root.programVisibilityRequested(!root.programVisible)
    }
}
