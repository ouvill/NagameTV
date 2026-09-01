import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import MirakurunViewer

ApplicationWindow {
    id: root
    width: 1280
    height: 760
    minimumWidth: 820
    minimumHeight: 520
    visible: true
    title: "Mirakurun Viewer"
    color: "#090d16"

    palette {
        window: "#090d16"
        windowText: "#eef3ff"
        base: "#141b2a"
        text: "#eef3ff"
        button: "#202a3d"
        buttonText: "#eef3ff"
        highlight: "#65d9c3"
    }

    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            GradientStop { position: 0; color: "#111827" }
            GradientStop { position: 1; color: "#070a10" }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 24
        spacing: 16

        RowLayout {
            Layout.fillWidth: true
            spacing: 14

            ColumnLayout {
                spacing: 0
                Label { text: "MIRAKURUN"; color: "#65d9c3"; font.pixelSize: 12; font.bold: true; font.letterSpacing: 2 }
                Label { text: "Live Viewer"; font.pixelSize: 26; font.bold: true }
            }
            Item { Layout.fillWidth: true }
            Rectangle {
                implicitWidth: statusLabel.implicitWidth + 24
                implicitHeight: 32
                radius: 16
                color: player.playing ? "#193d38" : "#202838"
                Label { id: statusLabel; anchors.centerIn: parent; text: player.status; color: player.playing ? "#7cf4dc" : "#b8c2d8" }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            radius: 18
            color: "#020409"
            border.color: "#263249"
            clip: true

            VideoItem { anchors.fill: parent }

            Label {
                anchors.centerIn: parent
                visible: !player.playing
                text: "Mirakurun のサービスを選んで再生"
                color: "#77839a"
                font.pixelSize: 18
            }
        }

        Rectangle {
            Layout.fillWidth: true
            implicitHeight: controls.implicitHeight + 28
            radius: 14
            color: "#151c2a"
            border.color: "#27334a"

            RowLayout {
                id: controls
                anchors.fill: parent
                anchors.margins: 14
                spacing: 12

                TextField {
                    Layout.preferredWidth: 290
                    placeholderText: "http://mirakurun:40772"
                    text: player.server
                    onEditingFinished: player.server = text
                }
                TextField {
                    Layout.preferredWidth: 190
                    placeholderText: "Service ID"
                    text: player.serviceId
                    inputMethodHints: Qt.ImhDigitsOnly
                    onEditingFinished: player.serviceId = text
                }
                Button { text: player.playing ? "再読み込み" : "再生"; highlighted: true; onClicked: player.play() }
                Button { text: "一時停止 / 再開"; enabled: player.playing; onClicked: player.togglePause() }
                Button { text: "停止"; enabled: player.playing; onClicked: player.stop() }
                Item { Layout.fillWidth: true }
                Label { text: "音量"; color: "#aeb9ce" }
                Slider {
                    Layout.preferredWidth: 150
                    from: 0; to: 100
                    value: player.volume
                    onMoved: player.volume = value
                }
            }
        }
    }
}
