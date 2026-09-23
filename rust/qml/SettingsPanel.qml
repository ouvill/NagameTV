pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs

Popup {
    id: root
    enum Page { Connection, Display, Comments, Shortcuts, Diagnostics, Remote, Timeshift }
    required property var backend
    readonly property var buildInfo: JSON.parse(backend.build_info)
    required property list<ShortcutBinding> shortcutEntries
    required property CommentSubmitPolicy commentSubmitPolicy
    required property Window targetWindow
    property bool statsVisible: false
    property int page: SettingsPanel.Connection
    property real pageReveal: 1
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property bool compact: width < 1200 || height < 700
    readonly property real navLeft: 24
    readonly property real navWidth: compact ? 184 : 208
    readonly property real navStep: 48
    readonly property real sideMargin: compact ? 32 : 48
    readonly property real contentStart: navLeft + navWidth + (compact ? 32 : 48)
    readonly property real pageWidth: Math.min(880, width - contentStart - sideMargin)
    readonly property real contentLeft: contentStart + Math.max(0, (width - contentStart - sideMargin - pageWidth) / 2)
    readonly property var categories: [
        qsTranslate("Settings", "Connection"), qsTranslate("Settings", "Display"),
        qsTranslate("Main", "Comments"), qsTranslate("Settings", "Shortcuts"),
        qsTranslate("Settings", "Diagnostics"), qsTranslate("Remote", "Remote control"), qsTranslate("Viewer", "Timeshift")
    ]
    signal statsRequested(bool visible)
    signal connectionAccepted
    signal recordingSearchReset
    signal modeRequested(int mode)
    function connectToServer() { connectionForm.connectToServer(); }
    function revealEpgstationConnection() {
        if (!root.visible || root.page !== SettingsPanel.Connection || !epgstationConnection.inputFocused) return;
        // A native resize may update the scaled overlay and nested layouts in
        // separate passes. Recalculate when either viewport or content settles.
        pages.ensurePolished();
        pageFlick.contentY = Math.min(epgstationConnection.mapToItem(pages, 0, 0).y, Math.max(0, pageFlick.contentHeight - pageFlick.height));
    }
    function focusEpgstationConnection() {
        page = SettingsPanel.Connection;
        Qt.callLater(function() {
            if (!root.visible || root.page !== SettingsPanel.Connection) return;
            epgstationConnection.focusInput();
            root.revealEpgstationConnection();
        });
    }
    onPageChanged: {
        pageFlick.contentY = 0;
        if (page === SettingsPanel.Comments) backend.comments_open(true);
        if (opened) pageRevealMotion.restart();
    }
    onAboutToHide: {
        epgstationConnection.closeLogin();
        liveBufferSettings.finishEdit();
        pageRevealMotion.complete();
    }
    onClosed: connectionForm.phase = ConnectionForm.Idle
    NumberAnimation {
        id: pageRevealMotion
        target: root; property: "pageReveal"
        from: 0; to: 1; duration: Theme.moveDuration
        easing.type: Easing.OutCubic
    }
    onAboutToShow: {
        connectionForm.reset();
        epgstationConnection.reset();
        remoteSettings.reset();
        languageError.visible = false;
        if (!backend.server.length) page = SettingsPanel.Connection;
        if (page === SettingsPanel.Comments) backend.comments_open(true);
    }
    parent: Overlay.overlay
    FolderDialog {
        id: screenshotFolderDialog
        objectName: "screenshotFolderDialog"
        title: qsTranslate("Settings", "Choose screenshot folder")
        onAccepted: root.backend.configure_screenshot_directory(selectedFolder)
    }
    x: 0; y: 0
    width: parent.width
    height: parent.height
    padding: 0
    modal: true
    dim: false
    focus: true
    closePolicy: Popup.CloseOnEscape
    background: Rectangle { color: Theme.surface }
    enter: Transition { ScreenFade { entering: true; from: 0; to: 1 } }
    exit: Transition { ScreenFade { entering: false; from: 1; to: 0 } }

    component Heading: Label {
        Layout.fillWidth: true
        color: Theme.textPrimary
        font.pixelSize: Theme.fontControl
        font.bold: true
        wrapMode: Text.Wrap
    }
    component Detail: Label {
        Layout.fillWidth: true
        textFormat: Text.PlainText
        color: Theme.textSecondary
        font.pixelSize: Theme.fontBody
        wrapMode: Text.Wrap
    }
    component Notice: Detail {
        padding: Theme.spaceLg
        background: Rectangle { color: Theme.warningSurface; radius: Theme.controlRadius }
    }
    component Problem: ColumnLayout {
        id: problem
        required property string message
        property string details: ""
        property bool expanded: false
        onDetailsChanged: expanded = false
        onVisibleChanged: if (!visible) expanded = false
        Layout.fillWidth: true
        spacing: Theme.spaceSm
        Notice { text: problem.message; color: Theme.warning }
        ActionButton {
            objectName: "problemDetailsToggle"
            visible: problem.details.length > 0
            text: problem.expanded ? qsTranslate("Settings", "Hide details") : qsTranslate("Settings", "Show details")
            emphasis: ActionButton.Quiet
            onClicked: problem.expanded = !problem.expanded
        }
        Detail {
            objectName: "problemDetails"
            visible: problem.expanded
            text: problem.details
            font.pixelSize: Theme.fontCaption
        }
    }
    component ShortcutRow: Item {
        id: shortcut
        required property string text
        required property string keys
        Layout.fillWidth: true
        implicitHeight: Math.max(52, shortcutRow.implicitHeight + 16)
        RowLayout {
            id: shortcutRow
            anchors { left: parent.left; right: parent.right; verticalCenter: parent.verticalCenter }
            spacing: Theme.spaceLg
            Label {
                Layout.fillWidth: true
                text: shortcut.text
                color: Theme.textPrimary
                font.pixelSize: Theme.fontControl
                wrapMode: Text.Wrap
            }
            Label {
                text: shortcut.keys
                color: Theme.accent
                font.pixelSize: Theme.fontBody
                padding: Theme.spaceSm
                background: Rectangle { radius: Theme.controlRadius; color: Theme.surfaceRaised }
            }
        }
        Rectangle {
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom }
            height: 1; color: Theme.surfaceRaised
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
            y: 80
            text: qsTranslate("Main", "Settings")
            color: Theme.textPrimary
            font.pixelSize: Theme.fontTitle
            font.bold: true
        }
        Flickable {
            id: navigation
            objectName: "settingsNavigation"
            x: root.navLeft
            y: 136
            width: root.navWidth
            height: Math.min(contentHeight, parent.height - y - root.navLeft)
            contentHeight: (root.categories.length - 1) * root.navStep + 40
            clip: true
            flickableDirection: Flickable.VerticalFlick
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: navigation.contentHeight > navigation.height ? ScrollBar.AsNeeded : ScrollBar.AlwaysOff }
            Rectangle {
                y: root.page * root.navStep
                width: parent.width; height: 40
                radius: Theme.controlRadius
                color: Theme.selection
                Behavior on y {
                    enabled: root.opened
                    NumberAnimation { duration: Theme.moveDuration; easing.type: Easing.OutCubic }
                }
            }
            Repeater {
                model: root.categories
                Button {
                    id: category
                    required property int index
                    required property string modelData
                    objectName: "settingsCategory" + index
                    y: index * root.navStep
                    width: navigation.width
                    height: 40
                    leftPadding: Theme.spaceLg; rightPadding: Theme.spaceLg
                    checked: root.page === index
                    Accessible.role: Accessible.PageTab
                    hoverEnabled: true
                    text: modelData
                    onClicked: root.page = index
                    contentItem: Label {
                        transform: Translate {
                            x: category.down ? 3 : category.hovered && !category.checked ? 2 : 0
                            Behavior on x { NumberAnimation { duration: Theme.colorDuration; easing.type: Easing.OutCubic } }
                        }
                        text: category.text
                        color: category.checked ? Theme.textPrimary : Theme.textSecondary
                        font.pixelSize: Theme.fontControl
                        font.bold: category.checked
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                        Behavior on color { ColorAnimation { duration: Theme.fadeInDuration } }
                    }
                    background: Rectangle {
                        radius: Theme.controlRadius
                        color: category.down ? Theme.selection : category.hovered && !category.checked ? Theme.overlayHover : "transparent"
                        border.color: category.visualFocus ? Theme.accent : "transparent"
                        Behavior on color { ColorAnimation { duration: Theme.colorDuration } }
                    }
                }
            }
        }
        Label {
            id: pageTitle
            opacity: 0.84 + 0.16 * root.pageReveal
            transform: Translate { y: 6 * (1 - root.pageReveal) }
            x: root.contentLeft
            y: 80
            width: root.pageWidth
            text: root.categories[root.page]
            color: Theme.textPrimary
            font.pixelSize: Theme.fontTitle
            font.bold: true
            wrapMode: Text.Wrap
        }
        ScrollView {
            id: pageScroll
            opacity: 0.84 + 0.16 * root.pageReveal
            transform: Translate { y: 10 * (1 - root.pageReveal) }
            objectName: "settingsScroll"
            width: root.pageWidth + rightPadding
            anchors { left: parent.left; leftMargin: root.contentLeft
                top: pageTitle.bottom; topMargin: 24; bottom: footer.top; bottomMargin: 20 }
            rightPadding: Theme.spaceXl
            clip: true
            ScrollBar.horizontal.policy: ScrollBar.AlwaysOff
            ScrollBar.vertical.policy: pageFlick.contentHeight > pageFlick.height ? ScrollBar.AlwaysOn : ScrollBar.AlwaysOff
            ScrollBar.vertical.palette.mid: Theme.textMuted
            contentItem: Flickable {
                id: pageFlick
                objectName: "settingsFlickable"
                contentWidth: width
                contentHeight: pages.implicitHeight
                onHeightChanged: Qt.callLater(root.revealEpgstationConnection)
                onContentHeightChanged: Qt.callLater(root.revealEpgstationConnection)
                boundsBehavior: Flickable.StopAtBounds
                ColumnLayout {
                    id: pages
                    width: pageFlick.width
                    spacing: Theme.spaceXl
                    Problem {
                        objectName: "settingsError"
                        visible: details.length > 0
                        message: qsTranslate("Settings", "Could not read or save settings. Your changes may not be available the next time you open the app.")
                        details: root.backend.settings_error
                    }
                    ColumnLayout {
                        visible: root.page === SettingsPanel.Connection
                        Layout.fillWidth: true
                        spacing: Theme.spaceLg
                        Heading { text: qsTranslate("Settings", "Mirakurun connection") }
                        ConnectionForm {
                            id: connectionForm
                            Layout.fillWidth: true
                            presentation: ConnectionForm.Settings
                            backend: root.backend
                            onCompleted: {
                                root.close();
                                root.connectionAccepted();
                            }
                        }
                        Detail { text: qsTranslate("Settings", "Changing the server stops playback and loads the new channel list.") }
                        SettingsToggle {
                            objectName: "autoplaySetting"
                            Layout.topMargin: 12
                            Layout.fillWidth: true
                            text: qsTranslate("Settings", "Play automatically on startup")
                            checked: root.backend.autoplay
                            onClicked: root.backend.configure_autoplay(checked)
                        }
                        LiveBufferSettings {
                            id: liveBufferSettings
                            Layout.topMargin: 12
                            Layout.fillWidth: true
                            backend: root.backend
                        }
                        Heading {
                            Layout.topMargin: Theme.spaceXl
                            text: qsTranslate("RecordingLibrary", "EPGStation connection")
                        }
                        EpgstationConnection {
                            id: epgstationConnection
                            objectName: "epgstationConnectionSettings"
                            Layout.fillWidth: true
                            serverUrl: root.backend.epgstation_server
                            busy: root.backend.epgstation_busy
                            connected: root.backend.epgstation_loaded
                            error: root.backend.epgstation_error
                            onConnectRequested: function(server) {
                                if (root.backend.browse_epgstation(server, "")) root.recordingSearchReset();
                            }
                            onLoginRequested: function(server, username, password) {
                                if (root.backend.login_epgstation(server, username, password)) root.recordingSearchReset();
                            }
                            onCancelRequested: root.backend.cancel_epgstation()
                            onRecordingsRequested: root.modeRequested(ModeNavigation.Recording)
                        }
                    }
                    Loader {
                        Layout.fillWidth: true
                        active: root.page === SettingsPanel.Timeshift
                        visible: active
                        sourceComponent: TimeshiftSettings { backend: root.backend }
                    }
                    RemoteSettings {
                        id: remoteSettings
                        visible: root.page === SettingsPanel.Remote
                        Layout.fillWidth: true
                        backend: root.backend
                    }
                    ColumnLayout {
                        visible: root.page === SettingsPanel.Display
                        Layout.fillWidth: true
                        spacing: Theme.spaceLg
                        RowLayout {
                            Layout.fillWidth: true
                            spacing: Theme.spaceXl
                            Detail {
                                text: qsTranslate("Settings", "Display language")
                                color: Theme.textPrimary
                                font.pixelSize: Theme.fontControl
                            }
                            SettingsChoice {
                                id: languageBox
                                objectName: "languageSetting"
                                Layout.preferredWidth: Math.min(280, pageFlick.width * 0.52)
                                dropdownIcon: root.iconDirectory + "chevron-down.svg"
                                Accessible.name: qsTranslate("Settings", "Display language")
                                model: [qsTranslate("Settings", "Use system language"), "日本語", "English"]
                                currentIndex: ["system", "ja", "en"].indexOf(root.backend.language)
                                onActivated: function(index) {
                                    languageError.visible = !root.backend.request_language(["system", "ja", "en"][index]);
                                    currentIndex = Qt.binding(function() { return ["system", "ja", "en"].indexOf(root.backend.language); });
                                }
                            }
                        }
                        Detail {
                            id: languageError
                            objectName: "languageError"
                            visible: false
                            text: qsTranslate("Settings", "Could not change the language. The previous language is still in use.")
                            color: Theme.warning
                        }
                        ColumnLayout {
                            Layout.topMargin: 8
                            Layout.fillWidth: true
                            spacing: 0
                            SettingsToggle {
                                objectName: "subtitleDisplay"
                                Layout.fillWidth: true
                                text: qsTranslate("Settings", "Show subtitles")
                                description: root.backend.subtitles_enabled
                                    ? ""
                                    : qsTranslate("Settings", "Subtitles are disabled by the launch options.")
                                checked: root.backend.subtitles_enabled && root.backend.subtitle_display
                                enabled: root.backend.subtitles_enabled
                                onClicked: root.backend.display_subtitles(checked)
                            }
                            SettingsToggle {
                                objectName: "subtitleForceOutline"
                                Layout.fillWidth: true
                                text: qsTranslate("Settings", "Always outline subtitles")
                                checked: root.backend.subtitle_force_outline
                                enabled: root.backend.subtitles_enabled
                                onClicked: root.backend.configure_subtitle_outline(checked)
                            }
                        }
                        Heading { Layout.topMargin: 20; text: qsTranslate("Settings", "Screenshots") }
                        Detail {
                            objectName: "screenshotDirectoryPath"
                            text: root.backend.screenshot_directory
                            color: Theme.textPrimary
                            wrapMode: Text.WrapAnywhere
                        }
                        Flow {
                            Layout.fillWidth: true
                            spacing: Theme.spaceMd
                            ActionButton {
                                objectName: "chooseScreenshotDirectory"
                                text: qsTranslate("Settings", "Change folder")
                                onClicked: {
                                    screenshotFolderDialog.currentFolder = root.backend.screenshot_directory_url();
                                    screenshotFolderDialog.open();
                                }
                            }
                            ActionButton {
                                objectName: "openScreenshotDirectory"
                                text: qsTranslate("Settings", "Open folder")
                                onClicked: root.backend.open_screenshot_directory()
                            }
                            ActionButton {
                                objectName: "resetScreenshotDirectory"
                                emphasis: ActionButton.Quiet
                                text: qsTranslate("Settings", "Use default folder")
                                onClicked: root.backend.reset_screenshot_directory()
                            }
                        }
                        ScreenshotFormatChoice {
                            Layout.fillWidth: true
                            backend: root.backend
                        }
                        Notice {
                            objectName: "screenshotFolderError"
                            visible: text.length > 0
                            text: root.backend.screenshot_error
                            color: Theme.warning
                        }
                    }
                    ColumnLayout {
                        visible: root.page === SettingsPanel.Comments
                        Layout.fillWidth: true
                        spacing: Theme.spaceLg
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            SettingsToggle {
                                objectName: "commentsEnabled"
                                Layout.fillWidth: true
                                text: qsTranslate("Settings", "Enable live comments")
                                description: root.backend.comments_allowed
                                    ? ""
                                    : qsTranslate("Settings", "Live comments are disabled by the launch options.")
                                checked: root.backend.comments_enabled === true
                                enabled: root.backend.comments_allowed === true
                                onClicked: root.backend.enable_comments(checked)
                            }
                            SettingsToggle {
                                objectName: "danmakuEnabled"
                                Layout.fillWidth: true
                                text: qsTranslate("Settings", "Show comments over the video")
                                checked: root.backend.danmaku_enabled === true
                                enabled: root.backend.comments_enabled === true
                                onClicked: root.backend.configure_danmaku(checked, root.backend.comment_font_size, root.backend.comment_opacity, root.backend.comment_speed)
                            }
                        }
                        Heading { Layout.topMargin: 20; text: qsTranslate("Settings", "Comment appearance") }
                        CommentPresentation {
                            Layout.fillWidth: true
                            enabled: root.backend.comments_enabled
                            displayMode: root.backend.comment_display
                            placementMode: root.backend.comment_placement
                            evaluationCollision: root.backend.evaluation_collision_layout
                            onSelected: function(display, placement) { root.backend.configure_comment_presentation(display, placement); }
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            enabled: root.backend.comments_enabled === true
                            spacing: 0
                            SettingsSlider {
                                objectName: "commentSize"
                                Layout.fillWidth: true
                                text: qsTranslate("Settings", "Text size")
                                valueText: Math.round(value) + " px"
                                from: 14; to: 72; stepSize: 1; value: root.backend.comment_font_size
                                onMoved: function(value) { root.backend.configure_danmaku(root.backend.danmaku_enabled, value, root.backend.comment_opacity, root.backend.comment_speed); }
                            }
                            SettingsSlider {
                                objectName: "commentOpacity"
                                Layout.fillWidth: true
                                text: qsTranslate("Settings", "Text opacity")
                                valueText: Math.round(value * 100) + "%"
                                from: 0; to: 1; stepSize: 0.05; value: root.backend.comment_opacity
                                onMoved: function(value) { root.backend.configure_danmaku(root.backend.danmaku_enabled, root.backend.comment_font_size, value, root.backend.comment_speed); }
                            }
                            SettingsSlider {
                                objectName: "commentSpeed"
                                Layout.fillWidth: true
                                text: qsTranslate("Main", "Speed")
                                valueText: value.toFixed(1) + "×"
                                from: 0.5; to: 2; stepSize: 0.1; value: root.backend.comment_speed
                                onMoved: function(value) { root.backend.configure_danmaku(root.backend.danmaku_enabled, root.backend.comment_font_size, root.backend.comment_opacity, value); }
                            }
                        }
                        SettingsToggle {
                            objectName: "commentShadow"
                            Layout.fillWidth: true
                            enabled: root.backend.comments_enabled
                            text: qsTranslate("Settings", "Drop shadow")
                            checked: root.backend.comment_shadow_enabled
                            onClicked: root.backend.configure_comment_shadow(checked)
                        }
                        RowLayout {
                            Layout.topMargin: 20
                            Layout.fillWidth: true
                            spacing: Theme.spaceXl
                            Detail {
                                text: qsTranslate("Settings", "Send with")
                                color: Theme.textPrimary
                                font.pixelSize: Theme.fontControl
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
                        ColumnLayout {
                            Layout.topMargin: 20
                            Layout.fillWidth: true
                            spacing: Theme.spaceSm
                            Heading { text: qsTranslate("Settings", "Saved comments") }
                            RowLayout {
                                Layout.fillWidth: true
                                Detail { text: qsTranslate("Settings", "Storage target (MiB)") }
                                ThemedSpinBox {
                                    objectName: "commentCacheLimit"
                                    from: 64; to: 65536; stepSize: 64
                                    editable: true
                                    value: root.backend.comment_cache_limit_mib
                                    Accessible.name: qsTranslate("Settings", "Storage target (MiB)")
                                    onValueModified: root.backend.configure_comment_cache_limit(value)
                                }
                            }
                            Detail {
                                objectName: "commentCacheUsage"
                                text: qsTranslate("Settings", "Disk usage: %1 MiB").arg((root.backend.comment_cache_bytes / (1024 * 1024)).toFixed(1))
                            }
                            Flow {
                                Layout.fillWidth: true
                                spacing: Theme.spaceSm
                                ActionButton {
                                    objectName: "refreshRecordingComments"
                                    text: qsTranslate("Settings", "Fetch this program's comments again")
                                    enabled: root.backend.recording && root.backend.comments_enabled && root.backend.danmaku_enabled
                                    onClicked: root.backend.refresh_recording_comments()
                                }
                                ActionButton {
                                    objectName: "clearCommentCache"
                                    text: qsTranslate("Settings", "Clear unused comments")
                                    onClicked: root.backend.clear_comment_cache()
                                }
                            }
                        }
                    }
                    ColumnLayout {
                        visible: root.page === SettingsPanel.Shortcuts
                        Layout.fillWidth: true
                        spacing: Theme.spaceXl
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 0
                            ShortcutRow {
                                text: qsTranslate("Settings", "Send a comment") + " · " + qsTranslate("Settings", "While entering a comment")
                                keys: root.commentSubmitPolicy.keys
                            }
                            Repeater {
                                model: root.shortcutEntries
                                delegate: ShortcutRow {
                                    required property ShortcutBinding modelData
                                    objectName: "shortcutHelp_" + modelData.objectName
                                    text: modelData.description + (modelData.condition.length ? " · " + modelData.condition : "")
                                    keys: modelData.nativeText
                                }
                            }
                        }
                    }
                    ColumnLayout {
                        visible: root.page === SettingsPanel.Diagnostics
                        Layout.fillWidth: true
                        spacing: Theme.spaceXl
                        SettingsToggle {
                            objectName: "statsVisible"
                            Layout.fillWidth: true
                            text: qsTranslate("Settings", "Show video statistics")
                            checked: root.statsVisible
                            onClicked: root.statsRequested(checked)
                        }
                        Heading { text: qsTranslate("Settings", "Build information") }
                        TextArea {
                            objectName: "buildInformation"
                            Layout.fillWidth: true
                            readOnly: true
                            selectByMouse: true
                            textFormat: TextEdit.PlainText
                            wrapMode: TextEdit.Wrap
                            color: Theme.textSecondary
                            selectionColor: Theme.surfacePressed
                            font.pixelSize: Theme.fontBody
                            padding: 0
                            background: null
                            text: [
                                qsTranslate("Settings", "Version: %1").arg(root.buildInfo.version),
                                qsTranslate("Settings", "Git commit: %1").arg(root.buildInfo.source.kind === "git"
                                    ? root.buildInfo.source.commit : qsTranslate("Settings", "Unavailable")),
                                qsTranslate("Settings", "Source state: %1").arg(root.buildInfo.source.kind === "git"
                                    ? (root.buildInfo.source.worktree === "clean" ? qsTranslate("Settings", "Clean") : qsTranslate("Settings", "Modified"))
                                    : qsTranslate("Settings", "Unavailable")),
                                qsTranslate("Settings", "Build time (UTC): %1").arg(new Date(root.buildInfo.built_unix_seconds * 1000).toISOString()),
                                qsTranslate("Settings", "Target: %1").arg(root.buildInfo.target),
                                qsTranslate("Settings", "Profile: %1").arg(root.buildInfo.profile),
                                qsTranslate("Settings", "Compiler: %1").arg(root.buildInfo.rustc),
                                qsTranslate("Settings", "Build features: %1").arg(root.buildInfo.features.length
                                    ? root.buildInfo.features.join(", ") : qsTranslate("Settings", "None"))
                            ].join("\n")
                        }
                        Heading { text: qsTranslate("Settings", "Logs") }
                        ActionButton {
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
            width: root.pageWidth
            anchors { left: parent.left; leftMargin: root.contentLeft
                bottom: parent.bottom; bottomMargin: 24 }
            height: 40
            spacing: Theme.spaceXl
            Label {
                Layout.fillWidth: true
                text: root.page === SettingsPanel.Diagnostics
                    ? qsTranslate("Settings", "Diagnostic display settings apply to this session only.")
                    : ""
                color: Theme.textMuted
                font.pixelSize: Theme.fontCaption
                wrapMode: Text.Wrap
            }
            ActionButton {
                objectName: "closeSettings"
                text: qsTranslate("Settings", "Back to viewing")
                onClicked: root.close()
            }
        }
        ModeNavigation {
            objectName: "settingsModeNavigation"
            targetWindow: root.targetWindow
            iconDirectory: root.iconDirectory
            mode: ModeNavigation.Settings
            guideEnabled: root.backend.epg_enabled
            onModeRequested: function(mode) { root.modeRequested(mode); }
        }
        Loader {
            anchors.fill: parent
            z: 1000
            active: root.targetWindow !== null
            sourceComponent: WindowResizeFrame { targetWindow: root.targetWindow }
        }
    }
}
