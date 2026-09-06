import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: root
    required property var backend
    property bool statsVisible: false
    signal statsRequested(bool visible)
    signal epgDisabled
    parent: Overlay.overlay
    anchors.centerIn: parent
    width: Math.min(620, parent.width - 48)
    padding: 24
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: Rectangle {
        color: "#151715"
        radius: 16
        border.color: "#38ffffff"
    }
    contentItem: ColumnLayout {
        spacing: 12
        RowLayout {
            Layout.fillWidth: true
            Label {
                text: "設定"
                color: "#f4f5f3"
                font.pixelSize: 23
                font.bold: true
            }
            Item {
                Layout.fillWidth: true
            }
            Button {
                text: "閉じる"
                onClicked: root.close()
            }
        }
        Label {
            text: "Mirakurun サーバー"
            color: "#b6bab6"
        }
        RowLayout {
            Layout.fillWidth: true
            TextField {
                id: server
                Layout.fillWidth: true
                text: root.backend.server
                placeholderText: "http://127.0.0.1:40772"
                onAccepted: root.backend.connect_server(text)
            }
            Button {
                text: root.backend.loading ? "取得中…" : "接続"
                enabled: !root.backend.loading
                onClicked: root.backend.connect_server(server.text)
            }
        }
        CheckBox {
            text: "字幕"
            palette.windowText: "#f4f5f3"
            checked: root.backend.subtitles_enabled
            enabled: root.backend.subtitles_allowed
            onClicked: root.backend.configure_features(checked, root.backend.epg_enabled)
        }
        CheckBox {
            text: "字幕を表示"
            palette.windowText: "#f4f5f3"
            checked: root.backend.subtitle_display
            enabled: root.backend.subtitles_enabled
            onClicked: root.backend.display_subtitles(checked)
        }
        CheckBox {
            text: "EPG"
            palette.windowText: "#f4f5f3"
            checked: root.backend.epg_enabled
            enabled: root.backend.epg_allowed
            onClicked: {
                root.backend.configure_features(root.backend.subtitles_enabled, checked);
                if (!checked)
                    root.epgDisabled();
            }
        }
        CheckBox {
            text: "動画統計"
            palette.windowText: "#f4f5f3"
            checked: root.statsVisible
            onClicked: root.statsRequested(checked)
        }
        Label {
            text: root.backend.settings_error
            visible: text.length > 0
            color: "#ffb080"
            Layout.fillWidth: true
            wrapMode: Text.Wrap
            Layout.leftMargin: 8
        }
        Label {
            text: root.backend.diagnostics
            color: "#aaaaaa"
            font.pixelSize: 11
            Layout.fillWidth: true
            Layout.leftMargin: 8
            elide: Text.ElideRight
        }
        Label {
            Layout.fillWidth: true
            Layout.leftMargin: 8
            Layout.rightMargin: 8
            Layout.bottomMargin: 8
            text: root.backend.status
            color: "white"
            elide: Text.ElideRight
        }
    }
}
