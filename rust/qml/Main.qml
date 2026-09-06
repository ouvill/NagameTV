import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.freedesktop.gstreamer.Qt6GLVideoItem 1.0
import MinimalViewer 1.0

ApplicationWindow {
    id: root
    visible: true
    width: 1100; height: 720
    minimumWidth: 720; minimumHeight: 480
    title: "Mirakurun Viewer — Feature Lab"
    color: "#151515"
    property bool closing: false
    property bool showGuide: false
    property bool showStats: false
    readonly property var channelRows: JSON.parse(player.channel_data)
    Player { id: player }
    function step(offset) {
        if (root.channelRows.length)
            player.select((player.selected + offset + root.channelRows.length) % root.channelRows.length)
    }
    Shortcut { sequence: "PgDown"; onActivated: root.step(1) }
    Shortcut { sequence: "PgUp"; onActivated: root.step(-1) }
    Timer { interval: 50; repeat: true; running: !root.closing; onTriggered: player.poll() }
    Timer { interval: 16; repeat: true; running: !root.closing && player.subtitles_active; onTriggered: player.poll_subtitles() }
    onClosing: { root.closing = true; player.shutdown() }
    Component.onCompleted: {
        if (player.attach(video) && player.server.length) player.connect_server(player.server)
    }
    ColumnLayout {
        anchors.fill: parent
        spacing: 6
        RowLayout {
            Layout.fillWidth: true; Layout.margins: 8
            TextField {
                id: server
                Layout.fillWidth: true
                text: player.server
                placeholderText: "http://127.0.0.1:40772"
                onAccepted: player.connect_server(text)
            }
            Button { text: player.loading ? "取得中…" : "接続"; enabled: !player.loading; onClicked: player.connect_server(server.text) }
        }
        RowLayout {
            Layout.fillWidth: true; Layout.leftMargin: 8
            CheckBox { palette.windowText: "#eeeeee"; text: "字幕"; checked: player.subtitles_enabled; enabled: player.subtitles_allowed; onClicked: player.configure_features(checked, player.epg_enabled) }
            CheckBox { palette.windowText: "#eeeeee"; text: "字幕を表示"; checked: player.subtitle_display; enabled: player.subtitles_enabled; onClicked: player.display_subtitles(checked) }
            Label { text: player.subtitles_enabled ? player.subtitle_status : "無効"; color: "#cccccc" }
            CheckBox { palette.windowText: "#eeeeee"; text: "EPG"; checked: player.epg_enabled; enabled: player.epg_allowed; onClicked: { player.configure_features(player.subtitles_enabled, checked); if (!checked) root.showGuide = false } }
            Button { text: root.showGuide ? "番組表を閉じる" : "番組表"; enabled: player.epg_enabled; onClicked: { root.showGuide = !root.showGuide; player.guide_open(root.showGuide) } }
            CheckBox { text: "動画統計"; palette.windowText: "#eeeeee"; checked: root.showStats; onClicked: root.showStats = checked }
            Item { Layout.fillWidth: true }
        }
        RowLayout {
            Layout.fillWidth: true; Layout.fillHeight: true
            Item {
                Layout.fillWidth: true; Layout.fillHeight: true
                GstGLQt6VideoItem { id: video; anchors.fill: parent }
                Loader {
                    anchors { top: parent.top; right: parent.right; margins: 8 }
                    width: Math.min(480, parent.width - 16)
                    active: !root.closing && root.showStats
                    z: 2
                    sourceComponent: Component { VideoStats { backend: player } }
                }
                Loader {
                    // Match a 16:9 broadcast's letterboxed video area.
                    anchors.centerIn: parent
                    width: Math.min(parent.width, parent.height * 16 / 9)
                    height: width * 9 / 16
                    active: !root.closing && player.subtitles_active && player.subtitle_display
                    sourceComponent: Component { SubtitleOverlay { captionJson: player.subtitle_data; outlineProvider: player } }
                }
            }
            Loader {
                Layout.preferredWidth: root.width * 0.40; Layout.fillHeight: true
                active: !root.closing && player.epg_enabled && root.showGuide
                visible: active
                sourceComponent: Component {
                    ProgramGuide {
                        programsJson: player.epg_data; status: player.epg_status
                        channel: player.selected >= 0 && player.selected < root.channelRows.length ? root.channelRows[player.selected].label : ""
                        onRefreshRequested: player.refresh_epg()
                        onCloseRequested: { root.showGuide = false; player.guide_open(false) }
                    }
                }
            }
        }
        RowLayout {
            Layout.fillWidth: true; Layout.margins: 8
            Button { text: "前"; enabled: root.channelRows.length > 0; onClicked: root.step(-1) }
            ChannelSelector {
                Layout.fillWidth: true
                rows: root.channelRows
                selected: player.selected
                onSelectRequested: function(index) { player.select(index) }
            }
            Button { text: "次"; enabled: root.channelRows.length > 0; onClicked: root.step(1) }
            Button { text: "再生"; enabled: player.selected >= 0; onClicked: player.play() }
            Button { text: "停止"; onClicked: player.stop() }
            Label { text: "音量"; color: "white" }
            Slider { from: 0; to: 1; value: player.volume_level; Layout.preferredWidth: 110; onMoved: player.volume(value) }
        }
        Label { text: player.settings_error; visible: text.length > 0; color: "#ffb080"; Layout.fillWidth: true; wrapMode: Text.Wrap; Layout.leftMargin: 8 }
        Label { text: player.diagnostics; color: "#aaaaaa"; font.pixelSize: 11; Layout.fillWidth: true; Layout.leftMargin: 8; elide: Text.ElideRight }
        Label {
            Layout.fillWidth: true
            Layout.leftMargin: 8; Layout.rightMargin: 8; Layout.bottomMargin: 8
            text: player.status; color: "white"; elide: Text.ElideRight
        }
    }
}
