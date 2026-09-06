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
    title: "Mirakurun Viewer — Minimal Rust"
    color: "#151515"
    property bool closing: false
    Player { id: player }
    function step(offset) {
        if (player.channels.length)
            player.select((player.selected + offset + player.channels.length) % player.channels.length)
    }
    Shortcut { sequence: "PgDown"; onActivated: root.step(1) }
    Shortcut { sequence: "PgUp"; onActivated: root.step(-1) }
    Timer { interval: 50; repeat: true; running: !root.closing; onTriggered: player.poll() }
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
        GstGLQt6VideoItem { id: video; Layout.fillWidth: true; Layout.fillHeight: true }
        RowLayout {
            Layout.fillWidth: true; Layout.margins: 8
            Button { text: "前"; enabled: player.channels.length > 0; onClicked: root.step(-1) }
            ComboBox {
                Layout.fillWidth: true
                model: player.channels
                currentIndex: player.selected
                onActivated: player.select(currentIndex)
            }
            Button { text: "次"; enabled: player.channels.length > 0; onClicked: root.step(1) }
            Button { text: "再生"; enabled: player.selected >= 0; onClicked: player.play() }
            Button { text: "停止"; onClicked: player.stop() }
            Label { text: "音量"; color: "white" }
            Slider { from: 0; to: 1; value: 0.5; Layout.preferredWidth: 110; onMoved: player.volume(value) }
        }
        Label {
            Layout.fillWidth: true
            Layout.leftMargin: 8; Layout.rightMargin: 8; Layout.bottomMargin: 8
            text: player.status; color: "white"; elide: Text.ElideRight
        }
    }
}
