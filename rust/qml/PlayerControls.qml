pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls

Item {
    id: root
    enum Density { Wide, Narrow, Dense }
    required property ViewerActions actions
    readonly property var backend: actions.backend
    property real videoWidth: width
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property bool screenshotHovered: screenshot.hovered
    readonly property bool transportControls: backend.recording || backend.timeshift
    readonly property int wideVideoWidth: 960
    readonly property int narrowVideoWidth: 740
    readonly property int density: videoWidth >= wideVideoWidth ? PlayerControls.Wide
        : videoWidth >= narrowVideoWidth ? PlayerControls.Narrow : PlayerControls.Dense
    readonly property int actionSize: density === PlayerControls.Dense ? 32 : 42
    readonly property int actionSpacing: density === PlayerControls.Dense ? 2 : 6
    readonly property int dividerWidth: density === PlayerControls.Dense ? 8 : 18
    readonly property alias audioAnchor: volume
    readonly property bool speedVisible: speedPanel.visible
    readonly property bool stacked: width < 620
    function closeSpeed() { speedPanel.close(); }
    enabled: actions.enabled
    implicitHeight: stacked ? 90 : 42
    onEnabledChanged: if (!enabled) { overflow.close(); speedPanel.close(); }
    onVisibleChanged: if (!visible) speedPanel.close()

    component Control: IconAction {
        flat: true
        implicitWidth: root.actionSize
        implicitHeight: root.actionSize
    }
    component Divider: Item {
        width: root.dividerWidth
        height: root.actionSize
        Rectangle {
            anchors.centerIn: parent
            width: 1; height: 24
            color: Theme.divider
        }
    }
    Row {
        id: leftControls
        objectName: "volumeControls"
        anchors { left: parent.left; verticalCenter: parent.verticalCenter; verticalCenterOffset: root.stacked ? 24 : 0 }
        spacing: Theme.spaceSm
        PlayerVolumeButton {
            id: volume
            actions: root.actions
            iconDirectory: root.iconDirectory
        }
        IconAction {
            objectName: "returnToLiveButton"
            flat: true
            visible: !root.backend.recording
            iconSource: root.iconDirectory + (root.actions.atLiveEdge ? "radio.svg" : "radio-off.svg")
            tip: action.text
            action: root.actions.returnToLive
        }
        ActionButton {
            id: speedButton
            surface: ActionButton.VideoOverlay
            selected: speedPanel.visible
            highlighted: root.backend.playback_rate !== 10
            objectName: "playbackSpeedButton"
            visible: root.backend.media_active
            implicitWidth: 58; implicitHeight: 42
            padding: Theme.spaceSm
            text: "x" + (root.backend.playback_rate / 10).toFixed(1)
            Accessible.name: qsTranslate("Viewer", "Playback speed: %1").arg(text)
            ThemedToolTip {
                visible: speedButton.hovered && !speedPanel.visible
                text: speedButton.Accessible.name
            }
            onClicked: speedPanel.toggle()
        }
    }
    PlaybackSpeedPanel {
        id: speedPanel
        objectName: "playbackSpeedPanel"
        backend: root.backend
        anchorItem: speedButton
        windowWidth: root.Overlay.overlay ? root.Overlay.overlay.width : root.width
        windowHeight: root.Overlay.overlay ? root.Overlay.overlay.height : root.height
        onActivity: root.actions.activity()
        onAboutToShow: root.actions.speedOpened()
    }
    Row {
        objectName: "transportControls"
        anchors.horizontalCenter: parent.horizontalCenter
        y: root.stacked ? 0 : (root.height - height) / 2
        spacing: Theme.spaceMd
        IconAction {
            objectName: "skipBackButton"
            flat: true
            visible: root.transportControls
            iconSource: root.iconDirectory + "rotate-ccw.svg"
            iconLabel: String(root.actions.seekSteps.backwardSeconds)
            tip: action.text
            action: root.actions.seekBackward
        }
        IconAction {
            objectName: "playStopButton"
            flat: true
            iconSource: root.iconDirectory + root.actions.playbackIcon
            tip: action.text
            action: root.actions.playbackToggle
        }
        IconAction {
            objectName: "skipForwardButton"
            flat: true
            visible: root.transportControls
            iconSource: root.iconDirectory + "rotate-cw.svg"
            iconLabel: String(root.actions.seekSteps.forwardSeconds)
            tip: action.text
            action: root.actions.seekForward
        }
    }
    Row {
        objectName: "viewControls"
        anchors { right: parent.right; verticalCenter: parent.verticalCenter; verticalCenterOffset: root.stacked ? 24 : 0 }
        Control {
            objectName: "channelsButton"
            visible: !root.backend.recording
            iconSource: root.iconDirectory + "grid-2x2.svg"
            tip: action.text
            action: root.actions.openChannels
        }
        Divider { visible: !root.backend.recording }
        Row {
            spacing: root.actionSpacing
            Control {
                objectName: "postCommentButton"
                visible: !root.backend.recording
                iconSource: root.iconDirectory + "pencil.svg"
                tip: action.text
                action: root.actions.openComposer
            }
            Control {
                id: screenshot
                objectName: "screenshotButton"
                iconSource: root.iconDirectory + "camera.svg"
                tip: action.text
                action: root.actions.captureScreenshot
            }
            Control {
                objectName: "subtitlesButton"
                visible: root.density === PlayerControls.Wide
                iconSource: root.iconDirectory + (root.backend.subtitle_display ? "captions.svg" : "captions-off.svg")
                tip: action.text
                active: root.backend.subtitles_enabled && root.backend.subtitle_display
                action: root.actions.toggleSubtitles
            }
            Control {
                objectName: "danmakuButton"
                visible: root.density === PlayerControls.Wide
                iconSource: root.iconDirectory + (root.backend.danmaku_enabled ? "message-square.svg" : "message-square-off.svg")
                tip: action.text
                active: enabled && root.backend.danmaku_enabled
                action: root.actions.toggleDanmaku
            }
            Control {
                objectName: "fullscreenButton"
                visible: root.density === PlayerControls.Wide
                iconSource: root.iconDirectory + "maximize.svg"
                tip: action.text
                action: root.actions.toggleFullscreen
            }
            Control {
                id: more
                objectName: "moreControlsButton"
                visible: root.density !== PlayerControls.Wide
                iconSource: root.iconDirectory + "ellipsis.svg"
                tip: qsTranslate("Viewer", "More controls")
                active: overflow.visible
                onClicked: if (overflow.visible) overflow.close(); else overflow.open()
                onVisibleChanged: if (!visible) overflow.close()
                Menu {
                    id: overflow
                    objectName: "playerOverflowMenu"
                    y: -height - 8
                    x: parent.width - width
                    width: 264
                    padding: Theme.spaceSm
                    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutsideParent
                    background: Rectangle { radius: Theme.panelRadius; color: Theme.popupSurface; border.color: Theme.divider }
                    component Entry: MenuItem {
                        id: menuEntry
                        implicitHeight: 48
                        icon.width: 24; icon.height: 24; icon.color: Theme.textPrimary
                        palette.text: Theme.textPrimary
                        palette.buttonText: Theme.textPrimary
                        background: Rectangle { radius: Theme.controlRadius; color: menuEntry.highlighted ? Theme.surfacePressed : "transparent" }
                    }
                    Entry {
                        objectName: "overflowSubtitles"
                        action: root.actions.toggleSubtitles
                        icon.source: root.iconDirectory + (root.backend.subtitle_display ? "captions.svg" : "captions-off.svg")
                    }
                    Entry {
                        objectName: "overflowDanmaku"
                        action: root.actions.toggleDanmaku
                        icon.source: root.iconDirectory + (root.backend.danmaku_enabled ? "message-square.svg" : "message-square-off.svg")
                    }
                    Entry {
                        objectName: "overflowFullscreen"
                        action: root.actions.toggleFullscreen
                        icon.source: root.iconDirectory + "maximize.svg"
                    }
                }
            }
        }
        Divider {}
        Control {
            objectName: "sidePanelButton"
            iconSource: root.iconDirectory + (root.actions.programVisible ? "panel-right-close.svg" : "panel-right-open.svg")
            tip: action.text
            action: root.actions.toggleProgram
        }
    }
}
