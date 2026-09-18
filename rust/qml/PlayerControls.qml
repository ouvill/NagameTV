pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: root
    required property ViewerActions actions
    readonly property var backend: actions.backend
    readonly property bool closing: !actions.enabled
    enabled: actions.enabled
    property bool settingsVisible: false
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property bool volumePressed: volumeSlider.pressed
    readonly property Item settingsButton: playbackSettingsButton
    // Keep all controls reachable when the sidebar narrows the video surface.
    readonly property bool transportControls: backend.recording || backend.timeshift === true
    readonly property bool compact: width < (transportControls ? 1000 : 780)
    readonly property int buttonSize: compact ? 36 : 42
    implicitHeight: compact ? 96 : 54
    component Control: IconAction {
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
        Control {
            objectName: "muteButton"
            iconSource: root.iconDirectory + (root.backend.audio_muted || root.backend.volume_level === 0 ? "volume-x.svg" : "volume-2.svg")
            tip: action.text
            action: root.actions.toggleMute
        }
        Control {
            objectName: "audioSelectionButton"
            iconSource: root.iconDirectory + "chevron-down.svg"
            tip: action.text
            implicitWidth: 24
            action: root.actions.openAudio
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
        Control {
            objectName: "channelsButton"
            visible: !root.backend.recording
            iconSource: root.iconDirectory + "grid-2x2.svg"
            tip: action.text
            action: root.actions.openChannels
        }
        Control {
            objectName: "skipBackButton"
            visible: root.transportControls
            iconSource: root.iconDirectory + "rotate-ccw.svg"
            iconLabel: String(root.actions.seekSteps.backwardSeconds)
            tip: action.text
            action: root.actions.seekBackward
        }
        Control {
            objectName: "playStopButton"
            iconSource: root.iconDirectory + root.actions.playbackIcon
            tip: action.text
            action: root.actions.playbackToggle
        }
        Control {
            objectName: "skipForwardButton"
            visible: root.transportControls
            iconSource: root.iconDirectory + "rotate-cw.svg"
            iconLabel: String(root.actions.seekSteps.forwardSeconds)
            tip: action.text
            action: root.actions.seekForward
        }
        Control {
            objectName: "postCommentButton"
            visible: !root.backend.recording
            iconSource: root.iconDirectory + "pencil.svg"
            tip: action.text
            action: root.actions.openComposer
        }
    }
    Row {
        objectName: "viewControls"
        anchors.right: parent.right
        y: root.compact ? 54 : 6
        spacing: 6
        Control {
            objectName: "screenshotButton"
            iconSource: root.iconDirectory + "camera.svg"
            tip: action.text
            action: root.actions.captureScreenshot
        }
        Control {
            objectName: "subtitlesButton"
            iconSource: root.iconDirectory + (root.backend.subtitle_display ? "captions.svg" : "captions-off.svg")
            tip: action.text
            active: root.backend.subtitles_enabled && root.backend.subtitle_display
            action: root.actions.toggleSubtitles
        }
        Control {
            objectName: "danmakuButton"
            iconSource: root.iconDirectory + (root.backend.danmaku_enabled ? "message-square.svg" : "message-square-off.svg")
            tip: action.text
            active: enabled && root.backend.danmaku_enabled
            action: root.actions.toggleDanmaku
        }
        Control {
            id: playbackSettingsButton
            objectName: "playbackSettingsButton"
            iconSource: root.iconDirectory + "settings-2.svg"
            tip: action.text
            active: root.settingsVisible
            action: root.actions.toggleSettings
        }
        Control {
            objectName: "fullscreenButton"
            iconSource: root.iconDirectory + "maximize.svg"
            tip: action.text
            action: root.actions.toggleFullscreen
        }
        Rectangle {
            width: 1
            height: 24
            anchors.verticalCenter: parent.verticalCenter
            color: "#42ffffff"
        }
        Control {
            objectName: "sidePanelButton"
            iconSource: root.iconDirectory + (root.actions.programVisible ? "panel-right-close.svg" : "panel-right-open.svg")
            tip: action.text
            action: root.actions.toggleProgram
        }
    }
}
