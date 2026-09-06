import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Drawer {
    id: root
    required property var backend
    property bool statsVisible: false
    signal statsRequested(bool visible)
    signal epgDisabled
    parent: Overlay.overlay
    property url collapseIcon: "qrc:/qt/qml/MinimalViewer/assets/icons/panel-right-close.svg"
    edge: Qt.RightEdge
    width: Math.min(420, parent.width * 0.88)
    height: parent.height
    leftPadding: 28
    rightPadding: 28
    topPadding: 28
    bottomPadding: 28
    dim: true
    modal: true
    focus: true
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
    background: Rectangle {
        color: "#fc151715"
        border.color: "#28ffffff"
    }
    contentItem: ScrollView {
        id: scroll
        clip: true
        contentWidth: availableWidth
        ColumnLayout {
            width: scroll.availableWidth
            spacing: 18
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
                IconAction {
                    objectName: "collapseSettings"
                    iconSource: root.collapseIcon
                    tip: "閉じる"
                    onClicked: root.close()
                }
            }
            Label {
                text: "Mirakurun サーバー"
                color: "#b6bab6"
            }
            TextField {
                id: server
                objectName: "serverField"
                Layout.fillWidth: true
                implicitHeight: 48
                text: root.backend.server
                color: "#f4f5f3"
                placeholderText: "http://mirakurun:40772"
                placeholderTextColor: "#8c918c"
                leftPadding: 16
                rightPadding: 16
                background: Rectangle {
                    radius: 12
                    color: "#1c1f1c"
                    border.color: server.activeFocus ? "#9caf9f" : "#30ffffff"
                }
                onAccepted: if (!root.backend.loading)
                    root.backend.connect_server(text)
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: "Mirakurun サーバーの URL を入力してください。"
                color: "#b6bab6"
            }
            Button {
                id: connect
                objectName: "connectServer"
                Layout.fillWidth: true
                text: root.backend.loading ? "取得中…" : "接続"
                enabled: !root.backend.loading
                contentItem: Label {
                    text: connect.text
                    color: "#17201a"
                    font.bold: true
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                background: Rectangle {
                    implicitHeight: 46
                    radius: 23
                    color: "#9caf9f"
                }
                onClicked: root.backend.connect_server(server.text)
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
}
