pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Popup {
    id: root
    enum Page { Connection, Display, Comments, Shortcuts, Diagnostics }
    required property var backend
    property Window targetWindow: null
    property bool statsVisible: false
    property int page: SettingsPanel.Connection
    property real pageReveal: 1
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property bool compact: width < 1200 || height < 700
    readonly property real navLeft: compact ? 24 : 36
    readonly property real navWidth: Math.min(310, Math.max(196, width * 0.2153))
    readonly property real contentLeft: navLeft + navWidth + (compact ? 40 : 64)
    readonly property real sideMargin: compact ? 32 : 80
    readonly property var categories: [
        qsTranslate("Settings", "Connection"), qsTranslate("Settings", "Display"),
        qsTranslate("Main", "Comments"), qsTranslate("Settings", "Shortcuts"),
        qsTranslate("Settings", "Diagnostics")
    ]
    signal statsRequested(bool visible)
    signal connectionAccepted
    function connectToServer() {
        if (backend.loading) return;
        connectionError.visible = !backend.connect_server(server.text);
        if (!connectionError.visible) {
            close();
            connectionAccepted();
        }
    }
    onPageChanged: {
        pageFlick.contentY = 0;
        if (opened) pageRevealMotion.restart();
    }
    onAboutToHide: pageRevealMotion.complete()
    NumberAnimation {
        id: pageRevealMotion
        target: root; property: "pageReveal"
        from: 0; to: 1; duration: 180
        easing.type: Easing.OutCubic
    }
    onAboutToShow: {
        server.text = backend.server;
        connectionError.visible = false;
        languageError.visible = false;
        if (!backend.server.length) page = SettingsPanel.Connection;
    }
    parent: Overlay.overlay
    x: 0; y: 0
    width: parent.width
    height: parent.height
    padding: 0
    modal: true
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape
    background: Rectangle { color: "#151715" }
    enter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 120 } }
    exit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 100 } }

    component Heading: Label {
        Layout.fillWidth: true
        color: "#f4f5f3"
        font.pixelSize: 21
        font.bold: true
        wrapMode: Text.Wrap
    }
    component Detail: Label {
        Layout.fillWidth: true
        textFormat: Text.PlainText
        color: "#b6bab6"
        font.pixelSize: 14
        wrapMode: Text.Wrap
    }
    component Notice: Detail {
        padding: 24
        background: Rectangle { color: "#24211d"; radius: 12; border.color: "#8c918c" }
    }
    component Problem: ColumnLayout {
        id: problem
        required property string message
        property string details: ""
        property bool expanded: false
        onDetailsChanged: expanded = false
        onVisibleChanged: if (!visible) expanded = false
        Layout.fillWidth: true
        spacing: 8
        Notice { text: problem.message; color: "#ffb080" }
        Button {
            id: detailsButton
            objectName: "problemDetailsToggle"
            visible: problem.details.length > 0
            text: problem.expanded ? qsTranslate("Settings", "Hide details") : qsTranslate("Settings", "Show details")
            onClicked: problem.expanded = !problem.expanded
            padding: 8
            contentItem: Label { text: detailsButton.text; color: "#9caf9f"; font.pixelSize: 13 }
            background: Rectangle { radius: 6; color: detailsButton.hovered ? "#1c1f1c" : "transparent"; border.color: detailsButton.visualFocus ? "#9caf9f" : "transparent" }
        }
        Detail {
            objectName: "problemDetails"
            visible: problem.expanded
            text: problem.details
            font.pixelSize: 13
        }
    }
    component Action: Button {
        id: action
        property bool primary: false
        property real feedbackScale: down ? 0.97 : 1
        Behavior on feedbackScale {
            NumberAnimation { duration: action.down ? 65 : 150; easing.type: Easing.OutCubic }
        }
        hoverEnabled: true
        implicitWidth: Math.max(180, contentItem.implicitWidth + 40)
        implicitHeight: 56
        leftPadding: 20; rightPadding: 20
        opacity: enabled ? 1 : 0.45
        contentItem: Label {
            scale: action.feedbackScale
            text: action.text
            color: action.primary ? "#0b0c0b" : "#f4f5f3"
            font.pixelSize: 16
            font.bold: true
            verticalAlignment: Text.AlignVCenter
            horizontalAlignment: Text.AlignHCenter
        }
        background: Rectangle {
            scale: action.feedbackScale
            radius: 10
            color: action.primary ? (action.down ? "#8da793" : action.hovered ? "#b6c6b8" : "#9caf9f")
                : (action.down ? "#344238" : action.hovered ? "#2b2926" : "#1c1f1c")
            border.color: action.visualFocus ? "#f4f5f3" : "#8c918c"
            Behavior on color { ColorAnimation { duration: 100 } }
        }
    }
    component ShortcutRow: Item {
        id: shortcut
        required property string text
        required property string keys
        Layout.fillWidth: true
        implicitHeight: Math.max(64, shortcutRow.implicitHeight + 24)
        RowLayout {
            id: shortcutRow
            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
            spacing: 16
            Label {
                Layout.fillWidth: true
                text: shortcut.text
                color: "#f4f5f3"
                font.pixelSize: 16
                wrapMode: Text.Wrap
            }
            Label {
                text: shortcut.keys
                color: "#9caf9f"
                font.pixelSize: 14
                padding: 8
                background: Rectangle { radius: 6; color: "#2b2926" }
            }
        }
        Rectangle {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: 1; color: "#1c1f1c"
        }
    }

    contentItem: Item {
        Loader {
            anchors { left: parent.left; right: parent.right; top: parent.top }
            height: 76
            active: root.targetWindow !== null
            sourceComponent: WindowDragArea { targetWindow: root.targetWindow }
        }
        Label {
            x: root.navLeft + 16
            y: root.compact ? 40 : 48
            text: qsTranslate("Main", "Settings")
            color: "#f4f5f3"
            font.pixelSize: root.compact ? 30 : 34
            font.bold: true
        }
        Item {
            id: navigation
            objectName: "settingsNavigation"
            x: root.navLeft
            y: root.compact ? 136 : 160
            width: root.navWidth
            height: root.categories.length * 64 - 16
            Rectangle {
                y: root.page * 64
                width: parent.width; height: 48
                radius: 10
                color: "#2b2926"
                border.color: "#8c918c"
                Behavior on y {
                    enabled: root.opened
                    NumberAnimation { duration: 180; easing.type: Easing.OutCubic }
                }
            }
            Repeater {
                model: root.categories
                Button {
                    id: category
                    required property int index
                    required property string modelData
                    objectName: "settingsCategory" + index
                    y: index * 64
                    width: navigation.width
                    height: 48
                    leftPadding: 26; rightPadding: 18
                    checked: root.page === index
                    Accessible.role: Accessible.PageTab
                    hoverEnabled: true
                    text: modelData
                    onClicked: root.page = index
                    contentItem: Label {
                        transform: Translate {
                            x: category.down ? 3 : category.hovered && !category.checked ? 2 : 0
                            Behavior on x { NumberAnimation { duration: 100; easing.type: Easing.OutCubic } }
                        }
                        text: category.text
                        color: category.checked ? "#f4f5f3" : "#b6bab6"
                        font.pixelSize: 18
                        font.weight: category.checked ? Font.DemiBold : Font.Normal
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                        Behavior on color { ColorAnimation { duration: 120 } }
                    }
                    background: Rectangle {
                        radius: 10
                        color: category.down ? "#289caf9f" : category.hovered && !category.checked ? "#10ffffff" : "transparent"
                        border.color: category.visualFocus ? "#9caf9f" : "transparent"
                        Behavior on color { ColorAnimation { duration: 100 } }
                    }
                }
            }
        }
        Label {
            id: pageTitle
            opacity: 0.84 + 0.16 * root.pageReveal
            transform: Translate { y: 6 * (1 - root.pageReveal) }
            x: root.contentLeft
            y: 72
            width: root.width - x - root.sideMargin
            text: root.page === SettingsPanel.Connection ? qsTranslate("Settings", "Mirakurun connection") : root.categories[root.page]
            color: "#f4f5f3"
            font.pixelSize: root.compact ? 26 : 30
            font.bold: true
            wrapMode: Text.Wrap
        }
        ScrollView {
            id: pageScroll
            opacity: 0.84 + 0.16 * root.pageReveal
            transform: Translate { y: 10 * (1 - root.pageReveal) }
            objectName: "settingsScroll"
            anchors { left: parent.left; leftMargin: root.contentLeft; right: parent.right; rightMargin: root.sideMargin - 20
                top: pageTitle.bottom; topMargin: 28; bottom: footer.top; bottomMargin: 28 }
            rightPadding: 20
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ScrollBar.vertical.policy: pageFlick.contentHeight > pageFlick.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
            ScrollBar.vertical.palette.mid: "#8c918c"
            contentItem: Flickable {
                id: pageFlick
                objectName: "settingsFlickable"
                contentWidth: width
                contentHeight: pages.implicitHeight
                boundsBehavior: Flickable.StopAtBounds
                ColumnLayout {
                    id: pages
                    width: pageFlick.width
                    spacing: 20
                    Problem {
                        objectName: "settingsError"
                        visible: details.length > 0
                        message: qsTranslate("Settings", "Could not read or save settings. Your changes may not be available the next time you open the app.")
                        details: root.backend.settings_error
                    }
                    ColumnLayout {
                        visible: root.page === SettingsPanel.Connection
                        Layout.fillWidth: true
                        spacing: 20
                        Detail { text: qsTranslate("Settings", "Server URL"); font.pixelSize: 16; font.bold: true }
                        Detail { text: qsTranslate("Settings", "Enter the Mirakurun server address starting with http:// or https://.") }
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: 20
                            TextField {
                                id: server
                                objectName: "serverField"
                                Layout.fillWidth: true
                                implicitHeight: 58
                                text: root.backend.server
                                color: "#f4f5f3"
                                font.pixelSize: 18
                                placeholderText: "http://mirakurun:40772"
                                placeholderTextColor: "#8c918c"
                                leftPadding: 20; rightPadding: 20
                                Accessible.name: qsTranslate("Settings", "Server URL")
                                background: Rectangle {
                                    radius: 10; color: "#2b2926"
                                    border.color: server.activeFocus ? "#9caf9f" : "#8c918c"
                                }
                                onAccepted: root.connectToServer()
                                onTextEdited: connectionError.visible = false
                            }
                            Action {
                                objectName: "connectServer"
                                primary: true
                                text: root.backend.loading ? qsTranslate("Settings", "Loading channels…") : qsTranslate("Main", "Save and connect")
                                enabled: !root.backend.loading
                                onClicked: root.connectToServer()
                            }
                        }
                        Detail { text: qsTranslate("Settings", "Changing the server stops playback and loads the new channel list.") }
                        Problem {
                            id: connectionError
                            objectName: "connectionError"
                            visible: false
                            message: qsTranslate("Settings", "Could not connect. Check the server URL and try again.")
                            details: root.backend.status
                        }
                    }
                    ColumnLayout {
                        visible: root.page === SettingsPanel.Display
                        Layout.fillWidth: true
                        spacing: 16
                        Heading { text: qsTranslate("Settings", "Display language") }
                        SettingsChoice {
                            id: languageBox
                            objectName: "languageSetting"
                            Layout.fillWidth: true
                            dropdownIcon: root.iconDirectory + "chevron-down.svg"
                            Accessible.name: qsTranslate("Settings", "Display language")
                            model: [qsTranslate("Settings", "Use system language"), "日本語", "English"]
                            currentIndex: ["system", "ja", "en"].indexOf(root.backend.language)
                            onActivated: function(index) {
                                languageError.visible = !root.backend.request_language(["system", "ja", "en"][index]);
                                currentIndex = Qt.binding(function() { return ["system", "ja", "en"].indexOf(root.backend.language); });
                            }
                        }
                        Detail {
                            id: languageError
                            objectName: "languageError"
                            visible: false
                            text: qsTranslate("Settings", "Could not change the language. The previous language is still in use.")
                            color: "#ffb080"
                        }
                        Heading { Layout.topMargin: 20; text: qsTranslate("Viewer", "Subtitles") }
                        SettingsToggle {
                            objectName: "subtitleDisplay"
                            Layout.fillWidth: true
                            text: qsTranslate("Settings", "Show subtitles")
                            description: root.backend.subtitles_enabled
                                ? qsTranslate("Settings", "Show subtitles when available in the program. You can also change this from the playback controls.")
                                : qsTranslate("Settings", "Subtitles are disabled by the launch options.")
                            checked: root.backend.subtitles_enabled && root.backend.subtitle_display
                            enabled: root.backend.subtitles_enabled
                            onClicked: root.backend.display_subtitles(checked)
                        }
                    }
                    ColumnLayout {
                        visible: root.page === SettingsPanel.Comments
                        Layout.fillWidth: true
                        spacing: 16
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            SettingsToggle {
                                objectName: "commentsEnabled"
                                Layout.fillWidth: true
                                text: qsTranslate("Settings", "Enable live comments")
                                description: root.backend.comments_allowed
                                    ? qsTranslate("Settings", "Receive and post NX-Jikkyo comments on supported channels.")
                                    : qsTranslate("Settings", "Live comments are disabled by the launch options.")
                                checked: root.backend.comments_enabled === true
                                enabled: root.backend.comments_allowed === true
                                onClicked: root.backend.enable_comments(checked)
                            }
                            SettingsToggle {
                                objectName: "danmakuEnabled"
                                Layout.fillWidth: true
                                text: qsTranslate("Settings", "Show comments over the video")
                                description: qsTranslate("Settings", "When off, you can still read the comment list and post comments.")
                                checked: root.backend.danmaku_enabled === true
                                enabled: root.backend.comments_enabled === true
                                onClicked: root.backend.configure_danmaku(checked, root.backend.comment_font_size, root.backend.comment_opacity, root.backend.comment_speed)
                            }
                        }
                        Heading { Layout.topMargin: 20; text: qsTranslate("Settings", "Comment appearance") }
                        ColumnLayout {
                            Layout.fillWidth: true
                            enabled: root.backend.comments_enabled === true
                            spacing: 0
                            SettingsSlider {
                                objectName: "commentSize"
                                Layout.fillWidth: true
                                text: qsTranslate("Settings", "Text size")
                                valueText: Math.round(value) + " px"
                                from: 14; to: 36; stepSize: 1; value: root.backend.comment_font_size
                                onMoved: function(value) { root.backend.configure_danmaku(root.backend.danmaku_enabled, value, root.backend.comment_opacity, root.backend.comment_speed); }
                            }
                            SettingsSlider {
                                objectName: "commentOpacity"
                                Layout.fillWidth: true
                                text: qsTranslate("Settings", "Text opacity")
                                valueText: Math.round(value * 100) + "%"
                                from: 0.2; to: 1; stepSize: 0.05; value: root.backend.comment_opacity
                                onMoved: function(value) { root.backend.configure_danmaku(root.backend.danmaku_enabled, root.backend.comment_font_size, value, root.backend.comment_speed); }
                            }
                            SettingsSlider {
                                objectName: "commentSpeed"
                                Layout.fillWidth: true
                                text: qsTranslate("Settings", "Scroll speed")
                                valueText: value.toFixed(1) + "×"
                                from: 0.5; to: 2; stepSize: 0.1; value: root.backend.comment_speed
                                onMoved: function(value) { root.backend.configure_danmaku(root.backend.danmaku_enabled, root.backend.comment_font_size, root.backend.comment_opacity, value); }
                            }
                        }
                        RowLayout {
                            Layout.topMargin: 20
                            Layout.fillWidth: true
                            spacing: 24
                            ColumnLayout {
                                Layout.fillWidth: true
                                spacing: 8
                                Heading { text: qsTranslate("Settings", "Comment posting") }
                                Detail {
                                    text: root.backend.comment_send_on_enter
                                        ? qsTranslate("Settings", "Press Enter to send. Ctrl + Enter also works.")
                                        : qsTranslate("Settings", "Press Ctrl + Enter to send. Enter alone does not send.")
                                }
                            }
                            SettingsChoice {
                                id: sendKey
                                objectName: "commentSendKey"
                                Layout.preferredWidth: Math.min(320, pageFlick.width * 0.46)
                                dropdownIcon: root.iconDirectory + "chevron-down.svg"
                                Accessible.name: qsTranslate("Settings", "Send with")
                                model: [qsTranslate("Settings", "Ctrl + Enter (default)"), "Enter"]
                                currentIndex: root.backend.comment_send_on_enter ? 1 : 0
                                onActivated: function(index) {
                                    root.backend.configure_comment_send_on_enter(index === 1);
                                    currentIndex = Qt.binding(function() { return root.backend.comment_send_on_enter ? 1 : 0; });
                                }
                            }
                        }
                    }
                    ColumnLayout {
                        visible: root.page === SettingsPanel.Shortcuts
                        Layout.fillWidth: true
                        spacing: 24
                        Detail { text: qsTranslate("Settings", "Shortcuts for watching TV. Channel and guide shortcuts are inactive while typing.") }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            ShortcutRow { text: qsTranslate("Settings", "Send a comment"); keys: root.backend.comment_send_on_enter ? "Enter / Ctrl + Enter" : "Ctrl + Enter" }
                            ShortcutRow { text: qsTranslate("Settings", "Toggle fullscreen"); keys: "F11" }
                            ShortcutRow { text: qsTranslate("Settings", "Open channels"); keys: "C" }
                            ShortcutRow { text: qsTranslate("Settings", "Open program guide"); keys: "G" }
                            ShortcutRow { text: qsTranslate("Settings", "Previous channel"); keys: "Page Up" }
                            ShortcutRow { text: qsTranslate("Settings", "Next channel"); keys: "Page Down" }
                            ShortcutRow { text: qsTranslate("Settings", "Close a panel or leave fullscreen"); keys: "Esc" }
                        }
                    }
                    ColumnLayout {
                        visible: root.page === SettingsPanel.Diagnostics
                        Layout.fillWidth: true
                        spacing: 24
                        SettingsToggle {
                            objectName: "statsVisible"
                            Layout.fillWidth: true
                            text: qsTranslate("Settings", "Show video statistics")
                            description: qsTranslate("Settings", "Show resolution, frame rate and playback performance while watching.")
                            checked: root.statsVisible
                            onClicked: root.statsRequested(checked)
                        }
                        Heading { text: qsTranslate("Settings", "Logs") }
                        Detail { text: qsTranslate("Settings", "Logs contain technical details to help investigate problems.") }
                        Action {
                            objectName: "openLogFolder"
                            text: qsTranslate("Main", "Open log folder")
                            onClicked: root.backend.open_log_folder()
                        }
                        Problem {
                            objectName: "logError"
                            message: qsTranslate("Settings", "Could not access the logs. Check the error details and try again.")
                            details: root.backend.log_error ? qsTranslate("Backend", root.backend.log_error) : ""
                            visible: details.length > 0
                        }
                        Heading { text: qsTranslate("Settings", "Current activity") }
                        Detail { text: root.backend.status; visible: text.length > 0 }
                        Detail {
                            objectName: "subtitleStatus"
                            text: qsTranslate("Settings", "Subtitles: %1").arg(root.backend.subtitle_status || "")
                            visible: root.backend.subtitles_enabled && (root.backend.subtitle_status || "").length > 0
                        }
                        Detail {
                            text: qsTranslate("Settings", "Live comments: %1").arg(root.backend.comment_status || "")
                            visible: (root.backend.comment_status || "").length > 0
                        }
                        Heading { text: qsTranslate("Settings", "Resource usage") }
                        Notice {
                            objectName: "featureDiagnostics"
                            text: root.backend.diagnostics
                            visible: text.length > 0
                        }
                    }
                }
            }
        }
        RowLayout {
            id: footer
            objectName: "settingsFooter"
            anchors { left: parent.left; leftMargin: root.contentLeft; right: parent.right; rightMargin: root.sideMargin
                bottom: parent.bottom; bottomMargin: root.compact ? 24 : 44 }
            height: 56
            spacing: 24
            Label {
                Layout.fillWidth: true
                text: root.page === SettingsPanel.Diagnostics
                    ? qsTranslate("Settings", "Diagnostic display settings apply to this session only.")
                    : root.page === SettingsPanel.Display || root.page === SettingsPanel.Comments
                        ? qsTranslate("Settings", "Changes take effect immediately.") : ""
                color: "#8c918c"
                font.pixelSize: 13
                wrapMode: Text.Wrap
            }
            Action {
                objectName: "closeSettings"
                text: qsTranslate("Settings", "Back to viewing")
                onClicked: root.close()
            }
        }
        Loader {
            anchors { right: parent.right; rightMargin: 20; top: parent.top; topMargin: 18 }
            width: 126; height: 42
            active: root.targetWindow !== null
            sourceComponent: WindowButtons { targetWindow: root.targetWindow; iconDirectory: root.iconDirectory }
        }
        Loader {
            anchors.fill: parent
            z: 1000
            active: root.targetWindow !== null
            sourceComponent: WindowResizeFrame { targetWindow: root.targetWindow }
        }
    }
}
