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
            Label {
                objectName: "subtitleStatus"
                text: root.backend.subtitle_status || ""
                visible: root.backend.subtitles_enabled && text.length > 0
                textFormat: Text.PlainText
                color: "#b6bab6"
                Layout.fillWidth: true
                wrapMode: Text.Wrap
            }
            CheckBox {
                text: "実況機能"
                palette.windowText: "#f4f5f3"
                checked: root.backend.comments_enabled === true
                enabled: root.backend.comments_allowed === true
                onClicked: root.backend.enable_comments(checked)
            }
            Label {
                text: root.backend.comment_status || ""
                visible: root.backend.comments_enabled === true
                textFormat: Text.PlainText
                color: "#b6bab6"
                Layout.fillWidth: true
                wrapMode: Text.Wrap
            }
            CheckBox {
                text: "画面に実況を流す"
                palette.windowText: "#f4f5f3"
                checked: root.backend.danmaku_enabled === true
                enabled: root.backend.comments_enabled === true
                onClicked: root.backend.configure_danmaku(checked, root.backend.comment_font_size, root.backend.comment_opacity, root.backend.comment_speed)
            }
            ColumnLayout {
                Layout.fillWidth: true
                enabled: root.backend.comments_enabled === true
                spacing: 12
                RowLayout {
                    Layout.fillWidth: true
                    Label { text: "文字サイズ"; color: "#b6bab6"; font.pixelSize: 11 }
                    Item { Layout.fillWidth: true }
                    Label { text: Math.round(root.backend.comment_font_size || 21) + " px"; color: "#f4f5f3"; font.pixelSize: 11 }
                }
                ThemedSlider {
                    Layout.fillWidth: true
                    from: 14; to: 36; stepSize: 1
                    value: root.backend.comment_font_size || 21
                    onMoved: root.backend.configure_danmaku(root.backend.danmaku_enabled, value, root.backend.comment_opacity, root.backend.comment_speed)
                }
                RowLayout {
                    Layout.fillWidth: true
                    Label { text: "透明度"; color: "#b6bab6"; font.pixelSize: 11 }
                    Item { Layout.fillWidth: true }
                    Label { text: Math.round((root.backend.comment_opacity || 1) * 100) + "%"; color: "#f4f5f3"; font.pixelSize: 11 }
                }
                ThemedSlider {
                    Layout.fillWidth: true
                    from: 0.2; to: 1; stepSize: 0.05
                    value: root.backend.comment_opacity || 1
                    onMoved: root.backend.configure_danmaku(root.backend.danmaku_enabled, root.backend.comment_font_size, value, root.backend.comment_speed)
                }
                RowLayout {
                    Layout.fillWidth: true
                    Label { text: "速度"; color: "#b6bab6"; font.pixelSize: 11 }
                    Item { Layout.fillWidth: true }
                    Label { text: (root.backend.comment_speed || 1).toFixed(1) + "×"; color: "#f4f5f3"; font.pixelSize: 11 }
                }
                ThemedSlider {
                    Layout.fillWidth: true
                    from: 0.5; to: 2; stepSize: 0.1
                    value: root.backend.comment_speed || 1
                    onMoved: root.backend.configure_danmaku(root.backend.danmaku_enabled, root.backend.comment_font_size, root.backend.comment_opacity, value)
                }
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
            Button {
                id: logFolder
                Layout.fillWidth: true
                text: "ログフォルダーを開く"
                onClicked: root.backend.open_log_folder()
                contentItem: Label {
                    text: logFolder.text
                    color: "#f4f5f3"
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                background: Rectangle {
                    implicitHeight: 40
                    radius: 12
                    color: logFolder.hovered ? "#1c1f1c" : "transparent"
                    border.color: "#30ffffff"
                }
            }
            Label {
                text: root.backend.log_error || ""
                visible: text.length > 0
                textFormat: Text.PlainText
                color: "#b6bab6"
                Layout.fillWidth: true
                wrapMode: Text.Wrap
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
