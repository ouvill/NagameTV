import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.freedesktop.gstreamer.Qt6GLVideoItem 1.0
import MirakurunViewer 1.0

ApplicationWindow {
    id: root
    width: 1280; height: 720
    minimumWidth: 820; minimumHeight: 480
    visible: true
    title: player.programName.length > 0
           ? player.programName + " — " + qsTr("Mirakurun Viewer")
           : qsTr("Mirakurun Viewer")
    color: "black"
    flags: Qt.Window | Qt.FramelessWindowHint
    property bool overlayVisible: true
    property bool overlayPinned: settingsPanel.opened

    Player { id: player }

    function revealOverlay() { overlayVisible = true; overlayTimer.restart() }
    function toggleFullScreen() {
        visibility = visibility === Window.FullScreen ? Window.Windowed : Window.FullScreen
        revealOverlay()
    }

    palette {
        window: "#0b111c"; windowText: "#f5f7fb"; base: "#121b2a"; text: "#f5f7fb"
        button: "#1c2738"; buttonText: "#f5f7fb"; highlight: "#62e6c4"
    }

    Shortcut { sequence: "Space"; enabled: player.playing; onActivated: player.togglePause() }
    Shortcut { sequence: "F11"; onActivated: root.toggleFullScreen() }
    Shortcut { sequence: "C"; onActivated: channelPanel.open() }
    Shortcut { sequence: "PgUp"; onActivated: player.changeChannel(-1) }
    Shortcut { sequence: "PgDown"; onActivated: player.changeChannel(1) }
    Shortcut {
        sequence: "Escape"
        onActivated: {
            if (settingsPanel.opened) settingsPanel.close()
            else if (root.visibility === Window.FullScreen) root.showNormal()
        }
    }

    Timer {
        id: overlayTimer
        interval: 3200
        onTriggered: if (player.playing && !root.overlayPinned) root.overlayVisible = false
    }

    Timer {
        interval: 50
        running: true
        repeat: true
        onTriggered: player.pollEvents()
    }

    GstGLQt6VideoItem { id: videoItem; objectName: "videoItem"; anchors.fill: parent }

    Rectangle {
        anchors.fill: parent
        visible: !player.playing
        color: "#0a101a"
        Column {
            anchors.centerIn: parent; spacing: 12
            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                text: "MIRAKURUN"; color: "#62e6c4"; font.pixelSize: 13
                font.bold: true; font.letterSpacing: 3
            }
            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Live television"); font.pixelSize: 34; font.weight: Font.DemiBold
            }
            Label {
                anchors.horizontalCenter: parent.horizontalCenter
                text: player.serviceId.length > 0 ? player.status : qsTr("Choose a service to begin")
                color: "#9daabd"; font.pixelSize: 16
            }
            Button {
                anchors.horizontalCenter: parent.horizontalCenter
                text: player.serviceId.length > 0 ? qsTr("Watch") : qsTr("Connection settings")
                highlighted: true
                onClicked: player.serviceId.length > 0 ? player.play() : settingsPanel.open()
            }
        }
    }

    // Future comments use pooled delegates in this independent layer, so
    // comment traffic never rebuilds or retains the video renderer.
    Item {
        objectName: "commentLayer"
        anchors.fill: parent
        anchors.topMargin: 64; anchors.bottomMargin: 150
        enabled: false
    }

    MouseArea {
        id: wakeArea
        anchors.fill: parent
        z: chrome.opacity > 0.01 ? -1 : 100
        acceptedButtons: Qt.AllButtons
        hoverEnabled: true
        onPositionChanged: root.revealOverlay()
        onPressed: root.revealOverlay()
    }

    Item {
        id: chrome
        anchors.fill: parent
        visible: opacity > 0
        opacity: root.overlayVisible || !player.playing ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 180 } }

        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0.0; color: "#b0000000" }
                GradientStop { position: 0.16; color: "#16000000" }
                GradientStop { position: 0.65; color: "#08000000" }
                GradientStop { position: 1.0; color: "#d9000000" }
            }
        }

        RowLayout {
            anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
            anchors.margins: 22; spacing: 10
            ColumnLayout {
                spacing: -2
                Label {
                    text: "MIRAKURUN"; color: "#62e6c4"; font.pixelSize: 11
                    font.bold: true; font.letterSpacing: 2.5
                }
                Label { text: qsTr("Live TV"); font.pixelSize: 22; font.weight: Font.DemiBold }
            }
            Rectangle {
                Layout.leftMargin: 8
                implicitWidth: connectionStatus.implicitWidth + 22; implicitHeight: 28; radius: 14
                color: player.playing ? "#a6195046" : "#991a2433"
                Label {
                    id: connectionStatus; anchors.centerIn: parent; text: player.status
                    color: player.playing ? "#8cf5db" : "#c5cfdd"; font.pixelSize: 12
                }
            }
            Item { Layout.fillWidth: true }
            ToolButton { text: "⚙"; font.pixelSize: 19; onClicked: settingsPanel.open() }
            ToolButton {
                text: root.visibility === Window.FullScreen ? "↙" : "↗"
                font.pixelSize: 19; onClicked: root.toggleFullScreen()
            }
            ToolButton { text: "×"; font.pixelSize: 24; onClicked: root.close() }
        }

        MouseArea {
            anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
            height: 72; acceptedButtons: Qt.LeftButton; z: -1
            onPressed: root.startSystemMove()
            onDoubleClicked: root.toggleFullScreen()
        }

        ColumnLayout {
            anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
            anchors.leftMargin: 30; anchors.rightMargin: 30; anchors.bottomMargin: 24; spacing: 14
            RowLayout {
                Layout.fillWidth: true; spacing: 18
                Label {
                    text: player.serviceId.length > 0 ? player.serviceId : "--"
                    color: "#62e6c4"; font.pixelSize: 14; font.bold: true
                }
                ColumnLayout {
                    Layout.fillWidth: true; spacing: 2
                    Label {
                        text: player.channelName.length > 0 ? player.channelName : qsTr("Live broadcast")
                        font.pixelSize: 21; font.weight: Font.DemiBold
                    }
                    Label {
                        text: player.programName.length > 0 ? player.programName : qsTr("Loading program information…")
                        color: "#dce3ec"; font.pixelSize: 14; elide: Text.ElideRight; Layout.fillWidth: true
                    }
                    Label {
                        text: player.programDescription
                        visible: text.length > 0; color: "#aab5c5"; font.pixelSize: 12
                        elide: Text.ElideRight; Layout.fillWidth: true
                    }
                }
                Label {
                    id: clockLabel
                    text: Qt.formatTime(new Date(), "hh:mm"); font.pixelSize: 18
                    Timer {
                        interval: 1000; running: true; repeat: true
                        onTriggered: clockLabel.text = Qt.formatTime(new Date(), "hh:mm")
                    }
                }
            }
            ProgressBar {
                Layout.fillWidth: true
                from: 0; to: 1; value: player.programProgress
                visible: player.programName.length > 0
            }
            RowLayout {
                Layout.fillWidth: true; spacing: 10
                RoundButton {
                    text: player.playing ? "Ⅱ" : "▶"; implicitWidth: 48; implicitHeight: 48
                    font.pixelSize: 18
                    onClicked: player.playing ? player.togglePause() : player.play()
                }
                ToolButton { text: "■"; enabled: player.playing; onClicked: player.stop() }
                Label { text: qsTr("Volume"); color: "#aab5c5" }
                Slider {
                    Layout.preferredWidth: 150; from: 0; to: 100; value: player.volume
                    onMoved: player.volume = value
                }
                Item { Layout.fillWidth: true }
                Button { text: qsTr("Channels"); onClicked: channelPanel.open() }
                Button { text: qsTr("Comments"); enabled: false }
            }
        }
    }

    Drawer {
        id: channelPanel
        edge: Qt.LeftEdge
        width: Math.min(430, root.width * 0.9); height: root.height
        modal: true; dim: true
        onOpened: { root.revealOverlay(); player.refreshChannels() }
        background: Rectangle { color: "#f50c1420"; border.color: "#27364a" }
        ColumnLayout {
            anchors.fill: parent; anchors.margins: 24; spacing: 14
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Channels"); font.pixelSize: 26; font.weight: Font.DemiBold }
                Item { Layout.fillWidth: true }
                ToolButton { text: "↻"; onClicked: player.refreshChannels() }
                ToolButton { text: "×"; font.pixelSize: 24; onClicked: channelPanel.close() }
            }
            Label {
                visible: player.services.length === 0
                text: qsTr("Loading channels…"); color: "#9daabd"
            }
            ListView {
                Layout.fillWidth: true; Layout.fillHeight: true
                clip: true; spacing: 4; model: player.services
                ScrollBar.vertical: ScrollBar { }
                delegate: ItemDelegate {
                    required property int index
                    required property string modelData
                    width: ListView.view.width; height: 54
                    highlighted: modelData === player.channelName
                    text: modelData; font.pixelSize: 16
                    onClicked: {
                        player.selectChannel(index)
                        channelPanel.close()
                    }
                }
            }
        }
    }

    Drawer {
        id: settingsPanel
        edge: Qt.RightEdge
        width: Math.min(420, root.width * 0.88); height: root.height
        modal: true; dim: true
        onOpened: root.revealOverlay()
        background: Rectangle { color: "#f50c1420"; border.color: "#27364a" }
        ColumnLayout {
            anchors.fill: parent; anchors.margins: 28; spacing: 18
            RowLayout {
                Layout.fillWidth: true
                Label { text: qsTr("Connection"); font.pixelSize: 26; font.weight: Font.DemiBold }
                Item { Layout.fillWidth: true }
                ToolButton { text: "×"; font.pixelSize: 24; onClicked: settingsPanel.close() }
            }
            Label { text: qsTr("Mirakurun server"); color: "#aab5c5" }
            TextField {
                id: serverField; Layout.fillWidth: true
                placeholderText: "http://mirakurun:40772"; text: player.server
            }
            Label { text: qsTr("Service ID"); color: "#aab5c5" }
            TextField {
                id: serviceField; Layout.fillWidth: true; placeholderText: "3203246080"
                text: player.serviceId; inputMethodHints: Qt.ImhDigitsOnly
            }
            Button {
                Layout.fillWidth: true; text: qsTr("Apply and watch"); highlighted: true
                onClicked: {
                    player.server = serverField.text
                    player.serviceId = serviceField.text
                    player.play()
                    settingsPanel.close()
                }
            }
            Label {
                Layout.fillWidth: true
                text: qsTr("Channel discovery and the program guide will use this server in a future step.")
                wrapMode: Text.WordWrap; color: "#7f8da1"
            }
            Item { Layout.fillHeight: true }
            Label {
                text: "F11  " + qsTr("Full screen") + "   Space  " + qsTr("Pause")
                color: "#7f8da1"
            }
        }
    }

    Component.onCompleted: {
        if (!player.attachVideoItem(videoItem))
            return
        player.refreshChannels()
        if (player.autoplay)
            player.play()
        overlayTimer.start()
    }
}
