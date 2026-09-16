pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Layouts

Item {
    id: root
    required property var backend
    property bool closing: false
    property bool canCapture: false
    property bool settingsVisible: false
    property bool sidePanelOpen: false
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property bool volumePressed: volumeSlider.pressed
    readonly property Item settingsButton: playbackSettingsButton
    // Keep all controls reachable when the sidebar narrows the video surface.
    readonly property bool compact: width < 780
    readonly property int buttonSize: compact ? 36 : 42
    implicitHeight: compact ? 96 : 54
    signal audioRequested
    signal channelsRequested
    signal commentRequested
    signal captureRequested
    signal settingsRequested
    signal fullscreenRequested
    signal sidePanelRequested

    component Action: IconAction {
        flat: true
        implicitWidth: root.buttonSize
        implicitHeight: root.buttonSize
    }
    RowLayout {
        id: volumeControls
        objectName: "volumeControls"
        anchors.left: parent.left
        y: root.compact ? 54 : 6
        height: root.buttonSize
        spacing: 4
        Action {
            objectName: "muteButton"
            iconSource: root.iconDirectory + (root.backend.audio_muted || root.backend.volume_level === 0 ? "volume-x.svg" : "volume-2.svg")
            tip: root.backend.audio_muted ? qsTranslate("Viewer", "Unmute") : qsTranslate("Viewer", "Mute")
            onClicked: root.backend.mute(!root.backend.audio_muted)
        }
        Action {
            objectName: "audioSelectionButton"
            iconSource: root.iconDirectory + "chevron-down.svg"
            tip: qsTranslate("Viewer", "Audio selection")
            implicitWidth: 24
            onClicked: root.audioRequested()
        }
        VolumeSlider {
            id: volumeSlider
            objectName: "playerVolumeSlider"
            Layout.preferredWidth: root.compact ? 96 : 132
            value: root.backend.volume_level
            subdued: root.backend.audio_muted
            closing: root.closing
            onVolumeRequested: function(fraction) { root.backend.volume(fraction); }
            onSaveRequested: root.backend.save_settings()
        }
    }
    Row {
        objectName: "transportControls"
        anchors.horizontalCenter: parent.horizontalCenter
        y: 6
        spacing: 12
        Action {
            objectName: "channelsButton"
            iconSource: root.iconDirectory + "grid-2x2.svg"
            tip: qsTranslate("Main", "Channels")
            onClicked: root.channelsRequested()
        }
        Action {
            objectName: "playStopButton"
            iconSource: root.iconDirectory + (root.backend.playing ? "square.svg" : "play-outline.svg")
            tip: root.backend.playing ? qsTranslate("Main", "Stop") : qsTranslate("Viewer", "Play")
            enabled: root.backend.playing || root.backend.recording || root.backend.selected >= 0
            onClicked: root.backend.playing ? root.backend.stop() : root.backend.play()
        }
        Action {
            objectName: "postCommentButton"
            iconSource: root.iconDirectory + "pencil.svg"
            tip: qsTranslate("Main", "Post a comment")
            enabled: root.backend.comments_enabled && !root.backend.recording
            onClicked: root.commentRequested()
        }
    }
    Row {
        objectName: "viewControls"
        anchors.right: parent.right
        y: root.compact ? 54 : 6
        spacing: 6
        Action {
            objectName: "screenshotButton"
            iconSource: root.iconDirectory + "camera.svg"
            tip: qsTranslate("Main", "Save screenshot")
            enabled: root.canCapture
            onClicked: root.captureRequested()
        }
        Action {
            objectName: "subtitlesButton"
            iconSource: root.iconDirectory + (root.backend.subtitle_display ? "captions.svg" : "captions-off.svg")
            tip: root.backend.subtitle_display ? qsTranslate("Main", "Hide subtitles") : qsTranslate("Main", "Show subtitles")
            active: root.backend.subtitles_enabled && root.backend.subtitle_display
            enabled: root.backend.subtitles_enabled
            onClicked: root.backend.display_subtitles(!root.backend.subtitle_display)
        }
        Action {
            objectName: "danmakuButton"
            iconSource: root.iconDirectory + (root.backend.danmaku_enabled ? "message-square.svg" : "message-square-off.svg")
            tip: root.backend.danmaku_enabled ? qsTranslate("Main", "Hide danmaku") : qsTranslate("Main", "Show danmaku")
            active: enabled && root.backend.danmaku_enabled
            enabled: root.backend.comments_enabled && !root.backend.recording
            onClicked: {
                root.backend.configure_danmaku(!root.backend.danmaku_enabled, root.backend.comment_font_size,
                    root.backend.comment_opacity, root.backend.comment_speed);
                root.backend.save_settings();
            }
        }
        Action {
            id: playbackSettingsButton
            objectName: "playbackSettingsButton"
            iconSource: root.iconDirectory + "settings-2.svg"
            tip: qsTranslate("Main", "Playback settings")
            active: root.settingsVisible
            onClicked: root.settingsRequested()
        }
        Action {
            objectName: "fullscreenButton"
            iconSource: root.iconDirectory + "maximize.svg"
            tip: qsTranslate("Main", "Fullscreen")
            onClicked: root.fullscreenRequested()
        }
        Rectangle {
            width: 1
            height: 24
            anchors.verticalCenter: parent.verticalCenter
            color: "#42ffffff"
        }
        Action {
            objectName: "sidePanelButton"
            iconSource: root.iconDirectory + (root.sidePanelOpen ? "panel-right-close.svg" : "panel-right-open.svg")
            tip: root.sidePanelOpen ? qsTranslate("Main", "Close side panel") : qsTranslate("Main", "Program information")
            onClicked: root.sidePanelRequested()
        }
    }
}
