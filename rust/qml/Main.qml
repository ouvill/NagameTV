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
    title: "Mirakurun Viewer — Feature Lab"
    color: "#0b0c0b"
    font.family: "Noto Sans CJK JP"
    property bool closing: false
    property bool showGuide: false
    property bool showChannels: false
    property bool showStats: false
    readonly property var channelRows: JSON.parse(player.channel_data)
    Player {
        id: player
    }
    function step(offset) {
        overlayVisibility.reveal();
        if (root.channelRows.length)
            player.select((player.selected + offset + root.channelRows.length) % root.channelRows.length);
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
        if (root.showChannels)
            root.showChannels = false;
        else if (root.showGuide)
            root.toggleGuide();
        else if (root.showStats)
            root.showStats = false;
        else
            windowActions.leaveFullscreen();
    }
    OverlayVisibility {
        id: overlayVisibility
        enabled: !root.closing
        playing: player.playing
        pinned: root.showChannels || root.showGuide || windowActions.popupOpen || windowActions.editingText || volumeSlider.pressed
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
    onClosing: {
        root.closing = true;
        player.shutdown();
    }
    Component.onCompleted: {
        surface.forceActiveFocus();
        if (player.attach(video) && player.server.length)
            player.connect_server(player.server);
        if (!player.server.length)
            settings.open();
    }
    Item {
        id: surface
        anchors.fill: parent
        focus: true
        signal activity
        onActivity: overlayVisibility.reveal()
        Component.onCompleted: player.observe_pointer(surface)
        GstGLQt6VideoItem {
            id: video
            anchors.fill: parent
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
            visible: overlayVisibility.controlsVisible
            IconAction {
                iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/calendar-days.svg"
                tip: "番組表"
                enabled: player.epg_enabled
                onClicked: root.toggleGuide()
            }
            IconAction {
                iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/settings-2.svg"
                tip: "設定"
                onClicked: settings.open()
            }
            WindowButtons {
                targetWindow: root
            }
        }
        SettingsDrawer {
            id: settings
            backend: player
            statsVisible: root.showStats
            onStatsRequested: function (visible) {
                root.showStats = visible;
            }
            onEpgDisabled: root.showGuide = false
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
                        tip: player.playing ? "停止" : "再生"
                        primary: !player.playing
                        enabled: player.playing || player.selected >= 0
                        onClicked: player.playing ? player.stop() : player.play()
                    }
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/" + (player.audio_muted || player.volume_level === 0 ? "volume-x.svg" : "volume-2.svg")
                        tip: player.audio_muted ? "消音解除" : "消音"
                        active: player.audio_muted
                        onClicked: player.mute(!player.audio_muted)
                    }
                    ThemedSlider {
                        id: volumeSlider
                        from: 0
                        to: 1
                        value: player.volume_level
                        subdued: player.audio_muted
                        Accessible.name: "音量"
                        Layout.preferredWidth: 132
                        onMoved: player.volume(value)
                    }
                    Item {
                        Layout.fillWidth: true
                    }
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/grid-2x2.svg"
                        tip: "チャンネル"
                        onClicked: root.showChannels = true
                    }
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/captions.svg"
                        tip: player.subtitle_display ? "字幕を非表示" : "字幕を表示"
                        active: player.subtitles_enabled && player.subtitle_display
                        enabled: player.subtitles_enabled
                        onClicked: player.display_subtitles(!player.subtitle_display)
                    }
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/settings-2.svg"
                        tip: "再生設定"
                        onClicked: settings.open()
                    }
                    IconAction {
                        iconSource: "qrc:/qt/qml/MinimalViewer/assets/icons/maximize.svg"
                        tip: "全画面"
                        onClicked: windowActions.toggleFullscreen()
                    }
                }
            }
        }
        Loader {
            anchors {
                top: parent.top
                topMargin: 150
                bottom: bottomPanel.top
                right: parent.right
            }
            width: Math.max(320, root.width * 0.40)
            z: 3
            active: !root.closing && player.epg_enabled && root.showGuide
            visible: active
            sourceComponent: Component {
                ProgramGuide {
                    programsJson: player.epg_data
                    status: player.epg_status
                    channel: player.selected >= 0 && player.selected < root.channelRows.length ? root.channelRows[player.selected].label : ""
                    onDayRequested: function (start, end) {
                        player.guide_day(start, end);
                    }
                    onRefreshRequested: player.refresh_epg()
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
            onActiveChanged: player.browser_open(active)
            z: 6
            onLoaded: item.focusBrowser()
            sourceComponent: ChannelBrowser {
                rows: root.channelRows
                programsJson: player.channel_program_data
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
            anchors {
                top: parent.top
                topMargin: 150
                right: parent.right
                margins: 8
            }
            width: Math.min(480, parent.width - 16)
            active: !root.closing && root.showStats
            z: 4
            sourceComponent: Component {
                VideoStats {
                    backend: player
                }
            }
        }
        WindowResizeFrame {
            anchors.fill: parent
            z: 1000
            targetWindow: root
            enabled: !root.closing
        }
    }
}
