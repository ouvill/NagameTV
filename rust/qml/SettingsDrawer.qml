pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Drawer {
    id: root
    required property var backend
    property bool statsVisible: false
    signal statsRequested(bool visible)
    signal connectionAccepted
    function connectToServer() {
        if (root.backend.loading) return;
        connectionError.visible = !root.backend.connect_server(server.text);
        if (!connectionError.visible) {
            root.close();
            root.connectionAccepted();
        }
    }
    signal epgDisabled
    parent: Overlay.overlay
    property url collapseIcon: "qrc:/qt/qml/MinimalViewer/assets/icons/panel-right-close.svg"
    property url dropdownIcon: "qrc:/qt/qml/MinimalViewer/assets/icons/chevron-down.svg"
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
                    text: qsTranslate("Main", "Settings")
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
                    tip: qsTranslate("Main", "Close")
                    onClicked: root.close()
                }
            }
            Label { text: qsTranslate("Main", "Language"); color: "#b6bab6" }
            ComboBox {
                id: languageBox
                Layout.fillWidth: true
                palette.button: "#1c1f1c"
                palette.buttonText: "#f4f5f3"
                palette.base: "#151715"
                palette.text: "#f4f5f3"
                palette.highlight: "#9caf9f"
                palette.highlightedText: "#17201a"
                implicitHeight: 46
                leftPadding: 14
                rightPadding: 40
                contentItem: Label {
                    text: languageBox.displayText
                    color: "#f4f5f3"
                    verticalAlignment: Text.AlignVCenter
                    elide: Text.ElideRight
                }
                indicator: Image {
                    x: languageBox.width - width - 14
                    y: (languageBox.height - height) / 2
                    width: 18; height: 18
                    source: root.dropdownIcon
                }
                background: Rectangle {
                    radius: 12
                    color: "#1c1f1c"
                    border.color: languageBox.activeFocus ? "#9caf9f" : "#30ffffff"
                }
                delegate: ItemDelegate {
                    id: languageOption
                    required property int index
                    required property string modelData
                    width: languageBox.width - 12
                    height: 42
                    highlighted: languageBox.highlightedIndex === index
                    contentItem: Label {
                        text: languageOption.modelData
                        color: "#f4f5f3"
                        font.bold: languageBox.currentIndex === languageOption.index
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                    background: Rectangle {
                        radius: 8
                        color: languageOption.highlighted ? "#389caf9f"
                            : languageBox.currentIndex === languageOption.index ? "#209caf9f" : "transparent"
                    }
                }
                popup: Popup {
                    y: languageBox.height + 6
                    width: languageBox.width
                    padding: 6
                    implicitHeight: contentItem.implicitHeight + topPadding + bottomPadding
                    background: Rectangle {
                        radius: 12
                        color: "#151715"
                        border.color: "#40ffffff"
                    }
                    contentItem: ListView {
                        clip: true
                        implicitHeight: contentHeight
                        model: languageBox.popup.visible ? languageBox.delegateModel : null
                        currentIndex: languageBox.highlightedIndex
                    }
                }
                model: [qsTranslate("Main", "System default"), "日本語", "English"]
                currentIndex: ["system", "ja", "en"].indexOf(root.backend.language)
                onActivated: function(index) {
                    languageError.visible = !root.backend.request_language(["system", "ja", "en"][index]);
                    // Restore the model binding even if Qt rejected the requested catalog.
                    languageBox.currentIndex = Qt.binding(function() { return ["system", "ja", "en"].indexOf(root.backend.language); });
                }
            }
            Label {
                id: languageError
                visible: false
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTranslate("Backend", "Could not load UI translation")
                color: "#b6bab6"
            }
            Label {
                text: qsTranslate("Viewer", "Mirakurun server")
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
                onAccepted: root.connectToServer()
            }
            Label {
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                text: qsTranslate("Viewer", "Enter your Mirakurun server URL.")
                color: "#b6bab6"
            }
            Button {
                id: connect
                objectName: "connectServer"
                Layout.fillWidth: true
                text: root.backend.loading ? qsTranslate("Viewer", "Loading\u2026") : qsTranslate("Main", "Save and connect")
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
                onClicked: root.connectToServer()
            }
            Label {
                id: connectionError
                objectName: "connectionError"
                visible: false
                Layout.fillWidth: true
                wrapMode: Text.Wrap
                textFormat: Text.PlainText
                text: root.backend.status
                color: "#b6bab6"
            }
            CheckBox {
                text: qsTranslate("Viewer", "Subtitles")
                palette.windowText: "#f4f5f3"
                checked: root.backend.subtitles_enabled
                enabled: root.backend.subtitles_allowed
                onClicked: root.backend.configure_features(checked, root.backend.epg_enabled)
            }
            CheckBox {
                text: qsTranslate("Main", "Show subtitles")
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
                text: qsTranslate("Viewer", "Comment reception")
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
                text: qsTranslate("Viewer", "Show comments over video")
                palette.windowText: "#f4f5f3"
                checked: root.backend.danmaku_enabled === true
                enabled: root.backend.comments_enabled === true
                onClicked: root.backend.configure_danmaku(checked, root.backend.comment_font_size, root.backend.comment_opacity, root.backend.comment_speed)
            }
            DanmakuAdjustments {
                Layout.fillWidth: true
                enabled: root.backend.comments_enabled === true
                textSize: root.backend.comment_font_size
                textOpacity: root.backend.comment_opacity
                speed: root.backend.comment_speed
                onAdjusted: function(size, opacity, speed) {
                    root.backend.configure_danmaku(root.backend.danmaku_enabled, size, opacity, speed);
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
                text: qsTranslate("Main", "Stats for nerds")
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
                text: qsTranslate("Main", "Open log folder")
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
