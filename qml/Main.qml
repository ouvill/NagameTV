import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.freedesktop.gstreamer.Qt6GLVideoItem 1.0
import MirakurunViewer 1.0

ApplicationWindow {
    id: root
    width: 1440; height: 900
    minimumWidth: 900; minimumHeight: 560
    visible: true
    title: player.programName.length ? player.programName : qsTr("Mirakurun Viewer")
    color: "#0b0c0b"
    flags: Qt.Window | Qt.FramelessWindowHint
    font.family: "Noto Sans CJK JP"

    readonly property color surface: "#151715"
    readonly property color raised: "#1c1f1c"
    readonly property color ink: "#f4f5f3"
    readonly property color muted: "#b6bab6"
    readonly property color accent: "#9caf9f"
    readonly property int panelWidth: Math.min(408, Math.max(360, width * .32))
    property string panel: ""
    property bool guideOpen: false
    property bool channelsOpen: false
    property alias danmaku: player.danmakuEnabled
    property alias commentFontSize: player.commentFontSize
    property alias commentOpacity: player.commentOpacity
    property alias commentSpeed: player.commentSpeed
    property bool overlayVisible: true
    property bool videoAttached: false
    property bool autoplayStarted: false
    property int selectedGuideIndex: -1
    property int guideDayOffset: 0
    property string guideType: "GR"
    property string channelPickerType: "GR"
    property double nowMs: Date.now()
    property double lastChannelRefreshMs: 0
    property var subtitleCue: null
    readonly property bool panelOpen: panel === "program" || panel === "comments" || panel === "channels"
    readonly property bool overlayPinned: guideOpen || channelsOpen || settings.opened || playbackSettings.opened

    Player { id: player }

    function availableChannelTypes() {
        const labels = { "GR": qsTr("地デジ"), "BS": "BS", "CS": "CS" }
        const result = []
        for (const type of ["GR", "BS", "CS"])
            if (player.channelTypes.indexOf(type) >= 0) result.push([type, labels[type]])
        return result
    }
    function normalizeChannelTypes() {
        const types = availableChannelTypes()
        if (types.length === 0) return
        if (!types.some(option => option[0] === guideType)) guideType = types[0][0]
        if (!types.some(option => option[0] === channelPickerType)) channelPickerType = types[0][0]
    }
    function channelIndices(channelType) {
        const result = []
        for (let i = 0; i < player.channelTypes.length; ++i)
            if (player.channelTypes[i] === channelType) result.push(i)
        return result
    }
    Connections { target: player; function onChannelTypesChanged() { root.normalizeChannelTypes() } }

    function scrollGuideToNow() {
        const position = 88 + (root.nowMs - guide.dayStart) / 60000 * guide.pixelsPerMinute
        const maximum = Math.max(0, epgFlick.contentHeight - epgFlick.height)
        epgFlick.contentY = Math.max(0, Math.min(maximum, position - epgFlick.height * 0.34))
    }
    onGuideOpenChanged: if (guideOpen) {
        guideDayOffset = 0
        selectedGuideIndex = -1
        Qt.callLater(root.scrollGuideToNow)
    }

    function reveal() { overlayVisible = true; hideTimer.restart() }
    function refreshChannelsIfDue(force) {
        const now = Date.now()
        if (!force && now - lastChannelRefreshMs < 60000) return
        lastChannelRefreshMs = now
        player.refreshChannels()
    }
    onPanelChanged: if (panel === "channels") refreshChannelsIfDue(false)
    onOverlayPinnedChanged: {
        if (!overlayPinned && player.playing) {
            overlayVisible = true
            hideTimer.restart()
        }
    }
    function scrollOneStep(view, event, horizontal, step) {
        const delta = event.angleDelta.y || event.angleDelta.x
        if (delta === 0) return

        view.cancelFlick()
        const position = horizontal ? view.contentX : view.contentY
        const contentSize = horizontal ? view.contentWidth : view.contentHeight
        const viewportSize = horizontal ? view.width : view.height
        const next = Math.max(0, Math.min(contentSize - viewportSize,
                                          position + (delta < 0 ? step : -step)))
        if (horizontal) view.contentX = next
        else view.contentY = next
        event.accepted = true
    }
    function toggleFullscreen() {
        visibility = visibility === Window.FullScreen ? Window.Windowed : Window.FullScreen
        reveal()
    }
    function closeTopmost() {
        if (playbackSettings.opened) playbackSettings.close()
        else if (settings.opened) settings.close()
        else if (selectedGuideIndex >= 0) selectedGuideIndex = -1
        else if (guideOpen) guideOpen = false
        else if (channelsOpen) channelsOpen = false
        else if (panelOpen) panel = ""
        else if (visibility === Window.FullScreen) showNormal()
    }
    function programTime(index) {
        if (index < 0 || index >= player.programStarts.length) return qsTr("番組情報なし")
        const start = Number(player.programStarts[index])
        const duration = Number(player.programDurations[index])
        if (!start || !duration) return qsTr("番組情報なし")
        return Qt.formatTime(new Date(start), "hh:mm") + "–" + Qt.formatTime(new Date(start + duration), "hh:mm")
    }
    function programProgressAt(index) {
        if (index < 0 || index >= player.programStarts.length) return 0
        const start = Number(player.programStarts[index])
        const duration = Number(player.programDurations[index])
        if (!start || duration <= 0) return 0
        return Math.max(0, Math.min(1, (root.nowMs - start) / duration))
    }
    function guideColor(genre) {
        const colors = ["#ffffe0", "#e0e0ff", "#ffe0f0", "#ffe0e0", "#e0ffe0", "#e0ffff", "#fff0e0", "#ffe0ff", "#ffffe0", "#fff0e0", "#e0f0ff", "#e0f0ff"]
        const value = Number(genre)
        return value >= 0 && value < colors.length ? colors[value] : "#f0f0f0"
    }
    function guideClock(milliseconds) { return Qt.formatTime(new Date(Number(milliseconds)), "hh:mm") }
    function uiIcon(name) { return "../assets/icons/" + name + ".svg" }
    function jikkyoForce(index) { return index < player.jikkyoForces.length && player.jikkyoForces[index].length ? qsTr("勢い ") + player.jikkyoForces[index] : "" }
    function guideChannelVisible(index) { return index < player.channelTypes.length && player.channelTypes[index] === guideType }
    function guideColumn(index) {
        let column = 0
        for (let i = 0; i < index; ++i) if (guideChannelVisible(i)) ++column
        return column
    }
    function guideChannelCount() {
        let count = 0
        for (let i = 0; i < player.channelTypes.length; ++i) if (guideChannelVisible(i)) ++count
        return count
    }

    Shortcut { sequence: "F11"; onActivated: root.toggleFullscreen() }
    Shortcut { sequence: "C"; onActivated: { channelsOpen = !channelsOpen; if (channelsOpen) refreshChannelsIfDue(false); reveal() } }
    Shortcut { sequence: "G"; onActivated: { guideOpen = !guideOpen; reveal() } }
    Shortcut { sequence: "PgUp"; onActivated: player.changeChannel(-1) }
    Shortcut { sequence: "PgDown"; onActivated: player.changeChannel(1) }
    Shortcut { sequence: "Escape"; onActivated: closeTopmost() }
    Timer { id: hideTimer; interval: 3200; onTriggered: if (player.playing && !overlayPinned) overlayVisible = false }
    Timer { interval: 50; running: true; repeat: true; onTriggered: player.pollEvents() }
    Timer { interval: 30000; running: true; repeat: true; triggeredOnStart: true; onTriggered: root.nowMs = Date.now() }
    Timer { interval: 1000; running: true; repeat: true; onTriggered: player.refreshCurrentPrograms() }
    Timer { interval: 300000; running: true; repeat: true; onTriggered: root.refreshChannelsIfDue(false) }
    onFrameSwapped: if (videoAttached && player.autoplay && !autoplayStarted) {
        autoplayStarted = true; Qt.callLater(function() { player.play() })
    }

    component RoundAction: Rectangle {
        id: action
        property url iconSource: ""
        property string tip: ""
        property bool primary: false
        property bool active: false
        signal triggered()
        implicitWidth: 42; implicitHeight: 42; radius: 21
        color: hover.containsMouse ? "#28ffffff" : (primary ? "#eeeeec" : (active ? "#389caf9f" : "#17000000"))
        border.color: primary ? "#80ffffff" : (active ? root.accent : "#16ffffff")
        Image { anchors.centerIn: parent; width: 24; height: 24; source: action.iconSource; visible: action.iconSource.toString().length > 0; sourceSize.width: 24; sourceSize.height: 24 }
        MouseArea { id: hover; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: action.triggered() }
        ToolTip {
            parent: action
            visible: hover.containsMouse && action.tip.length > 0
            text: action.tip
            delay: 150
            timeout: 3000
            x: (action.width - implicitWidth) / 2
            y: -implicitHeight - 10
            padding: 9
            contentItem: Label { text: action.tip; color: root.ink; font.pixelSize: 12 }
            background: Rectangle { radius: 8; color: "#e61b1d1b"; border.color: "#38ffffff" }
        }
    }

    component ToggleSwitch: Rectangle {
        id: toggle
        property bool checked: false
        signal toggled()
        implicitWidth: 42; implicitHeight: 24; radius: 12
        color: checked ? root.accent : "#4c4f4c"
        Behavior on color { ColorAnimation { duration: 120 } }
        Rectangle {
            width: 18; height: 18; radius: 9
            x: toggle.checked ? toggle.width - width - 3 : 3
            anchors.verticalCenter: parent.verticalCenter
            color: toggle.checked ? "#17201a" : "#d7d9d7"
            Behavior on x { NumberAnimation { duration: 120; easing.type: Easing.OutCubic } }
        }
        MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: toggle.toggled() }
    }
    component WindowAction: Rectangle {
        id: windowAction
        required property url iconSource
        property bool destructive: false
        signal triggered()
        width: 42; height: 42; radius: 21
        color: hover.containsMouse ? (destructive ? "#a94b3f" : "#28ffffff") : "transparent"
        Behavior on color { ColorAnimation { duration: 100 } }
        Image { anchors.centerIn: parent; width: 16; height: 16; source: windowAction.iconSource }
        MouseArea { id: hover; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: windowAction.triggered() }
    }
    component BroadcastTabs: Rectangle {
        id: broadcastTabs
        property string value: "GR"
        readonly property var options: root.availableChannelTypes()
        readonly property int selectedIndex: Math.max(0, options.findIndex(option => option[0] === value))
        signal selected(string channelType)
        implicitWidth: Math.max(82, options.length * 76 + 6); implicitHeight: 40; radius: 20
        visible: options.length > 0
        color: "#b8171918"; border.color: "#32ffffff"
        readonly property real segmentWidth: (width - 6) / Math.max(1, options.length)
        Rectangle {
            x: 3 + broadcastTabs.selectedIndex * broadcastTabs.segmentWidth
            y: 3; width: broadcastTabs.segmentWidth; height: parent.height - 6; radius: height / 2
            color: "#429caf9f"; border.color: root.accent
            Behavior on x { NumberAnimation { duration: 170; easing.type: Easing.OutCubic } }
        }
        Row { x: 3; width: parent.width - 6; height: parent.height
            Repeater { model: broadcastTabs.options
                Item { required property var modelData; width: broadcastTabs.segmentWidth; height: broadcastTabs.height
                    Label { anchors.centerIn: parent; text: modelData[1]; color: broadcastTabs.value === modelData[0] ? root.ink : "#d5d8d5"; font.bold: broadcastTabs.value === modelData[0] }
                    MouseArea { anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: broadcastTabs.selected(modelData[0]) }
                }
            }
        }
    }
    component WindowButtons: Rectangle {
        implicitWidth: 126; implicitHeight: 42; radius: 21
        color: "#b8171819"; border.color: "#16ffffff"
        Row { anchors.fill: parent
            WindowAction { iconSource: root.uiIcon("minus"); onTriggered: root.showMinimized() }
            WindowAction { iconSource: root.uiIcon("square"); onTriggered: root.visibility === Window.Maximized ? root.showNormal() : root.showMaximized() }
            WindowAction { iconSource: root.uiIcon("x"); destructive: true; onTriggered: root.close() }
        }
    }

    Rectangle { anchors.fill: parent; color: "#0b0c0b" }
    Item {
        id: videoRegion
        width: root.panelOpen ? root.width - root.panelWidth : root.width
        height: root.height
        GstGLQt6VideoItem {
            id: videoItem; objectName: "videoItem"
            width: parent.width
            height: root.panelOpen ? Math.min(parent.height, width * 9 / 16) : parent.height
            anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
        }
        Rectangle {
            anchors.fill: videoItem; visible: !player.playing; color: "#141516"
            Column {
                anchors.centerIn: parent; spacing: 14
                Label { anchors.horizontalCenter: parent.horizontalCenter; text: qsTr("ライブテレビ"); color: root.ink; font.pixelSize: 32; font.bold: true }
                Label { anchors.horizontalCenter: parent.horizontalCenter; text: player.serviceId.length ? player.status : qsTr("視聴するチャンネルを選択してください"); color: root.muted }
                Rectangle { anchors.horizontalCenter: parent.horizontalCenter; width: 148; height: 44; radius: 22; color: root.accent; Label { anchors.centerIn: parent; text: player.serviceId.length ? qsTr("視聴する") : qsTr("接続設定"); color: "#191a1b"; font.bold: true } MouseArea { anchors.fill: parent; onClicked: player.serviceId.length ? player.play() : settings.open() } }
            }
        }
        Item {
            id: danmakuLayer
            width: videoItem.width
            height: Math.min(videoItem.height, width * 9 / 16)
            anchors.horizontalCenter: videoItem.horizontalCenter
            anchors.verticalCenter: videoItem.verticalCenter
            visible: root.danmaku && player.playing
            clip: true
            property var laneEntries: [null, null, null, null, null, null, null, null]
            readonly property bool titleOverlapsVideo: persistentProgramIdentity.visible
                && persistentProgramIdentity.y < videoItem.y + y + height
                && persistentProgramIdentity.y + persistentProgramIdentity.height > videoItem.y + y
            readonly property real titleBottomInVideo: persistentProgramIdentity.y
                + persistentProgramIdentity.height - videoItem.y - y
            readonly property real laneTop: titleOverlapsVideo
                ? Math.max(40, Math.min(height * .4, titleBottomInVideo + 16)) : 40
            readonly property real laneSpacing: Math.max(30, Math.min(58, (height - laneTop - 50) / 8))
            function selectLane(entry, speed) {
                const startX = width + entry.implicitWidth
                let earliestLane = 0
                let earliestRight = Number.MAX_VALUE
                for (let lane = 0; lane < laneEntries.length; ++lane) {
                    const previous = laneEntries[lane]
                    if (previous === null) {
                        laneEntries[lane] = entry
                        return lane
                    }
                    const previousRight = previous.x + previous.width
                    const gap = startX - previousRight
                    const catchesBeforeExit = speed > previous.motionSpeed
                        && gap / (speed - previous.motionSpeed) < previousRight / previous.motionSpeed
                    if (gap >= 24 && !catchesBeforeExit) {
                        laneEntries[lane] = entry
                        return lane
                    }
                    if (previousRight < earliestRight) {
                        earliestRight = previousRight
                        earliestLane = lane
                    }
                }
                laneEntries[earliestLane] = entry
                return earliestLane
            }
            function releaseLane(entry) {
                if (entry.lane >= 0 && laneEntries[entry.lane] === entry)
                    laneEntries[entry.lane] = null
            }
            Component { id: danmakuComment
                Label { id: danmakuEntry; required property string commentText; property int lane: -1; property real motionSpeed: 0; text: commentText; width: implicitWidth; y: danmakuLayer.laneTop + lane * danmakuLayer.laneSpacing; color: root.ink; opacity: root.commentOpacity; font.pixelSize: root.commentFontSize; font.bold: true; style: Text.Outline; styleColor: "#d0000000"
                    Behavior on y { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
                    NumberAnimation { id: danmakuMotion; target: danmakuEntry; property: "x"; easing.type: Easing.Linear; onFinished: { danmakuLayer.releaseLane(danmakuEntry); danmakuEntry.destroy() } }
                    Component.onCompleted: {
                        const textWidth = implicitWidth
                        const visibleDuration = 9000 / root.commentSpeed
                        const visibleDistance = danmakuLayer.width + textWidth
                        const pixelsPerMillisecond = visibleDistance / visibleDuration
                        motionSpeed = pixelsPerMillisecond
                        lane = danmakuLayer.selectLane(danmakuEntry, motionSpeed)
                        danmakuMotion.from = danmakuLayer.width + textWidth
                        danmakuMotion.to = -textWidth
                        danmakuMotion.duration = Math.round((danmakuMotion.from - danmakuMotion.to) / pixelsPerMillisecond)
                        x = danmakuMotion.from
                        danmakuMotion.start()
                    }
                }
            }
        }
        Connections { target: player
            function onCommentReceived(text) {
                if (!root.danmaku || !player.playing || text.length === 0) return
                danmakuComment.createObject(danmakuLayer, { "commentText": text })
            }
        }
        Item {
            anchors.fill: danmakuLayer; z: 20
            visible: player.playing && player.subtitlesEnabled
                && (player.subtitleText.length > 0 || player.subtitleData.length > 0)
            clip: true
            Repeater {
                model: root.subtitleCue && root.subtitleCue.cells ? root.subtitleCue.cells : []
                delegate: Rectangle {
                    id: subtitleCell
                    required property var modelData
                    readonly property real scaleX: parent.width / Math.max(1, root.subtitleCue.planeWidth)
                    readonly property real scaleY: parent.height / Math.max(1, root.subtitleCue.planeHeight)
                    x: modelData.x * scaleX
                    y: modelData.y * scaleY
                    width: Math.max(1, modelData.width * scaleX)
                    height: Math.max(1, modelData.height * scaleY)
                    color: modelData.background
                    Label {
                        id: subtitleGlyph
                        anchors.centerIn: parent
                        text: modelData.text
                        color: modelData.foreground
                        font.family: root.font.family
                        font.pixelSize: Math.max(8, modelData.glyphHeight * subtitleCell.scaleY)
                        font.bold: modelData.bold
                        font.italic: modelData.italic
                        font.underline: modelData.underline
                        renderType: Text.NativeRendering
                        style: modelData.stroked ? Text.Outline : Text.Normal
                        styleColor: modelData.stroke
                        transform: Scale {
                            origin.x: subtitleGlyph.width / 2
                            origin.y: subtitleGlyph.height / 2
                            xScale: subtitleGlyph.implicitWidth > 0
                                ? Math.min(1, modelData.glyphWidth * subtitleCell.scaleX / subtitleGlyph.implicitWidth) : 1
                        }
                    }
                }
            }
            Label {
                visible: !root.subtitleCue || !root.subtitleCue.cells || root.subtitleCue.cells.length === 0
                anchors.horizontalCenter: parent.horizontalCenter
                anchors.bottom: parent.bottom
                anchors.bottomMargin: controls.opacity > 0 ? 126 : 38
                width: Math.min(parent.width * .82, 1040)
                text: player.subtitleText
                color: "white"; font.pixelSize: 28; font.bold: true
                horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap
                style: Text.Outline; styleColor: "#e0000000"
                Behavior on anchors.bottomMargin { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } }
            }
        }
    }

    Timer {
        id: subtitleClearTimer
        interval: 7000
        onTriggered: { player.subtitleText = ""; player.subtitleData = ""; root.subtitleCue = null }
    }
    Connections {
        target: player
        function onSubtitleDataChanged() {
            if (player.subtitleData.length === 0) {
                root.subtitleCue = null
                subtitleClearTimer.stop()
                return
            }
            try {
                root.subtitleCue = JSON.parse(player.subtitleData)
                subtitleClearTimer.interval = Math.max(100, Math.min(60000, root.subtitleCue.durationMs || 7000))
                subtitleClearTimer.restart()
            } catch (error) {
                console.warn("Could not parse subtitle regions:", error)
            }
        }
    }

    Column {
        id: persistentProgramIdentity
        z: 402
        visible: controls.opacity > 0 && !root.guideOpen
        anchors.left: videoRegion.left; anchors.top: videoRegion.top; anchors.margins: 24
        width: Math.max(360, videoRegion.width - 430); spacing: 8
        Row { spacing: 12
            Item { width: 64; height: 36
                Image { id: currentChannelLogo; anchors.fill: parent; source: player.channelLogoUrl; fillMode: Image.PreserveAspectFit; asynchronous: true; cache: true }
                Label { anchors.centerIn: parent; visible: currentChannelLogo.status !== Image.Ready; text: qsTr("局ロゴ"); color: root.muted; font.pixelSize: 10 }
            }
            Label { anchors.verticalCenter: parent.verticalCenter; text: player.channelName.length ? player.channelName.replace(/^\d+\s+/, "") : qsTr("チャンネル"); color: root.muted; font.pixelSize: 13; style: Text.Outline; styleColor: "#90000000" }
        }
        Label {
            width: parent.width
            text: player.programName.length ? player.programName : qsTr("番組情報なし")
            color: root.ink; font.pixelSize: 23; font.bold: true
            wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight
            style: Text.Outline; styleColor: "#a0000000"
            MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: root.panel = "program" }
        }
        Label { text: root.programTime(Math.max(0, player.services.indexOf(player.channelName))); color: "#d7d7d6"; font.pixelSize: 12; style: Text.Outline; styleColor: "#90000000" }
    }

    MouseArea { anchors.fill: videoRegion; z: controls.opacity > .01 ? -1 : 100; acceptedButtons: Qt.AllButtons; hoverEnabled: true; onPositionChanged: reveal(); onPressed: reveal() }
    Item {
        id: controls; anchors.fill: videoRegion; visible: opacity > 0 && !guideOpen; z: 401
        opacity: overlayVisible || !player.playing ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 180 } }
        Rectangle { anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; height: Math.min(210, parent.height * .28)
            gradient: Gradient { GradientStop { position: 0; color: "#a8000000" } GradientStop { position: 1; color: "#00000000" } }
        }
        Rectangle { anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; height: Math.min(360, parent.height * .46); visible: !root.channelsOpen
            gradient: Gradient { GradientStop { position: 0; color: "#00000000" } GradientStop { position: 1; color: "#d6000000" } }
        }
        Row {
            visible: !root.panelOpen
            anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 18; spacing: 14
            RoundAction { iconSource: root.uiIcon("calendar-days"); tip: qsTr("番組表"); onTriggered: { guideOpen = true; reveal() } }
            RoundAction { iconSource: root.uiIcon("settings-2"); tip: qsTr("設定"); onTriggered: settings.open() }
            WindowButtons {}
        }
        MouseArea { anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; height: 76; acceptedButtons: Qt.LeftButton; z: -1; onPressed: root.startSystemMove(); onDoubleClicked: root.visibility === Window.Maximized ? root.showNormal() : root.showMaximized() }
        Column {
            id: playerControlBar
            visible: opacity > 0; enabled: !root.channelsOpen
            opacity: root.channelsOpen ? 0 : 1
            transform: Translate { y: root.channelsOpen ? 20 : 0; Behavior on y { NumberAnimation { duration: 180; easing.type: Easing.OutCubic } } }
            Behavior on opacity { NumberAnimation { duration: 130; easing.type: Easing.OutCubic } }
            anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
            anchors.leftMargin: 24; anchors.rightMargin: 24; anchors.bottomMargin: 22; spacing: 12
            Row { width: parent.width; Label { text: Math.round(player.programProgress * 100) + "%"; color: root.muted; font.pixelSize: 11 } }
            ProgressBar { width: parent.width; height: 4; from: 0; to: 1; value: player.programProgress; background: Rectangle { implicitHeight: 3; radius: 2; color: "#42ffffff" } contentItem: Item { Rectangle { width: parent.width * player.programProgress; height: 3; radius: 2; color: "#e1e1df" } } }
            RowLayout {
                width: parent.width; spacing: 12
                RoundAction { iconSource: root.uiIcon("square"); tip: qsTr("停止"); onTriggered: player.stop() }
                RoundAction { iconSource: root.uiIcon("volume-2"); tip: qsTr("音量") }
                Slider { Layout.preferredWidth: 132; from: 0; to: 100; value: player.volume; onMoved: player.volume = value; onPressedChanged: if (!pressed) player.saveSettings() }
                Item { Layout.fillWidth: true }
                RoundAction { iconSource: root.uiIcon("grid-2x2"); tip: qsTr("チャンネル"); onTriggered: { channelsOpen = true; root.refreshChannelsIfDue(false); reveal() } }
                RoundAction { iconSource: root.uiIcon("pencil"); tip: qsTr("コメント投稿") }
                RoundAction { iconSource: root.uiIcon("captions"); tip: player.subtitlesEnabled ? qsTr("字幕を非表示") : qsTr("字幕を表示"); active: player.subtitlesEnabled; onTriggered: { player.subtitlesEnabled = !player.subtitlesEnabled; if (!player.subtitlesEnabled) { player.subtitleText = ""; player.subtitleData = ""; root.subtitleCue = null }; player.saveSettings() } }
                RoundAction { iconSource: root.uiIcon("settings-2"); tip: qsTr("再生設定"); onTriggered: { playbackSettings.open(); root.reveal() } }
                RoundAction { iconSource: root.uiIcon("maximize"); tip: qsTr("全画面"); onTriggered: root.toggleFullscreen() }
                Rectangle { width: 1; height: 28; color: "#28ffffff"; Layout.leftMargin: 4; Layout.rightMargin: 4; Layout.alignment: Qt.AlignVCenter }
                RoundAction { iconSource: root.uiIcon(root.panelOpen ? "panel-right-close" : "panel-right-open"); tip: root.panelOpen ? qsTr("サイドパネルを閉じる") : qsTr("サイドパネルを開く"); onTriggered: root.panel = root.panelOpen ? "" : "program" }
            }
        }
    }

    Rectangle {
        id: sidePanel
        x: root.panelOpen ? root.width - root.panelWidth : root.width
        width: root.panelWidth; height: root.height
        visible: true; enabled: root.panelOpen; clip: true; color: root.surface
        Behavior on x { NumberAnimation { duration: 220; easing.type: Easing.OutCubic } }
        Rectangle { width: 1; height: parent.height; color: "#20ffffff" }
        Row { anchors.right: parent.right; anchors.rightMargin: 18; anchors.top: parent.top; anchors.topMargin: 18; spacing: 10
            RoundAction { iconSource: root.uiIcon("calendar-days"); tip: qsTr("番組表"); onTriggered: root.guideOpen = true }
            RoundAction { iconSource: root.uiIcon("settings-2"); tip: qsTr("設定"); onTriggered: settings.open() }
            WindowButtons {}
        }
        ColumnLayout {
            width: root.panelWidth - 48; height: parent.height - 102; x: 24; y: 78; spacing: 14
            RowLayout {
                Layout.fillWidth: true
                RoundAction { iconSource: root.uiIcon("panel-right-close"); tip: qsTr("折りたたむ"); onTriggered: root.panel = "" }
                Label { text: root.panel === "comments" ? qsTr("コメント") : (root.panel === "channels" ? qsTr("チャンネル") : qsTr("番組情報")); color: root.ink; font.pixelSize: 17; font.bold: true }
                Item { Layout.fillWidth: true }
                Row { visible: root.panel === "comments"; spacing: 9; Layout.alignment: Qt.AlignVCenter
                    Label { height: parent.height; verticalAlignment: Text.AlignVCenter; text: qsTr("弾幕"); color: root.danmaku ? root.ink : root.muted; font.pixelSize: 13 }
                    ToggleSwitch { checked: root.danmaku; onToggled: { root.danmaku = !root.danmaku; player.saveSettings() } }
                }
            }
            ColumnLayout {
                visible: root.panel === "program"; Layout.fillWidth: true; spacing: 13
                RowLayout { Item { width: 56; height: 32; Image { id: panelLogo; anchors.fill: parent; source: player.channelLogoUrl; fillMode: Image.PreserveAspectFit; asynchronous: true; cache: true } Label { anchors.centerIn: parent; visible: panelLogo.status !== Image.Ready; text: qsTr("局ロゴ"); color: root.muted; font.pixelSize: 10 } } Label { text: player.channelName.length ? player.channelName : qsTr("チャンネル"); color: root.ink; font.weight: Font.DemiBold } }
                Label { Layout.fillWidth: true; text: player.programName; color: root.ink; font.pixelSize: 23; font.bold: true; wrapMode: Text.Wrap }
                Label { text: root.programTime(Math.max(0, player.services.indexOf(player.channelName))); color: "#d4d4d3"; font.pixelSize: 13 }
                ProgressBar {
                    Layout.fillWidth: true; height: 4; value: player.programProgress
                    background: Rectangle { implicitHeight: 3; radius: 2; color: "#30ffffff" }
                    contentItem: Item { Rectangle { width: parent.width * player.programProgress; height: 3; radius: 2; color: root.accent } }
                }
                Label { text: qsTr("概要"); color: root.muted; font.weight: Font.DemiBold }
                Label { Layout.fillWidth: true; text: player.programDescription.length ? player.programDescription : qsTr("番組概要はありません"); color: "#e4e4e3"; font.pixelSize: 15; wrapMode: Text.Wrap; lineHeight: 1.35 }
                Rectangle { Layout.fillWidth: true; height: 1; color: "#18ffffff" }
                Label { Layout.fillWidth: true; text: qsTr("Mirakurunから取得した番組情報を表示しています"); color: "#929497"; font.pixelSize: 12; wrapMode: Text.Wrap }
            }
            ListView {
                id: commentList; visible: root.panel === "comments"; Layout.fillWidth: true; Layout.fillHeight: true; clip: true
                model: player.commentTexts
                onCountChanged: if (count > 0) positionViewAtEnd()
                delegate: Item { required property int index; required property string modelData; width: ListView.view.width; height: Math.max(62, commentBody.implicitHeight + 28)
                    Label { x: 0; y: 16; width: 62; text: index < player.commentTimes.length ? player.commentTimes[index] : ""; color: "#929497"; font.pixelSize: 10 }
                    Label { id: commentBody; x: 70; y: 13; width: parent.width - 70; text: modelData; color: "#e5e5e4"; font.pixelSize: 14; wrapMode: Text.Wrap }
                    Label { anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.bottomMargin: 5; text: index < player.commentSources.length ? player.commentSources[index] : ""; color: root.muted; font.pixelSize: 9 }
                    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: "#12ffffff" }
                }
                Label { anchors.centerIn: parent; visible: commentList.count === 0; width: parent.width - 24; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; text: player.commentStatus; color: root.muted; font.pixelSize: 13 }
            }
            Rectangle { visible: root.panel === "comments"; Layout.fillWidth: true; height: 58; radius: 20; color: root.raised; border.color: "#606163"; Label { anchors.left: parent.left; anchors.leftMargin: 18; anchors.verticalCenter: parent.verticalCenter; text: qsTr("コメントを入力…"); color: "#9fa0a2" } RoundAction { anchors.right: parent.right; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; iconSource: "../assets/icons/send.svg"; tip: qsTr("送信") } }
            Item { visible: root.panel === "channels"; Layout.fillWidth: true; Layout.fillHeight: true
                BroadcastTabs { id: sideChannelTabs; anchors.top: parent.top; anchors.horizontalCenter: parent.horizontalCenter; value: root.channelPickerType; onSelected: function(channelType) { root.channelPickerType = channelType; sideChannelList.positionViewAtBeginning() } }
                ListView { id: sideChannelList; anchors.left: parent.left; anchors.right: parent.right; anchors.top: sideChannelTabs.bottom; anchors.bottom: parent.bottom; anchors.topMargin: 14; spacing: 12; clip: true; model: root.channelIndices(root.channelPickerType); boundsBehavior: Flickable.DragAndOvershootBounds; boundsMovement: Flickable.FollowBoundsBehavior
                    delegate: Rectangle { required property var modelData; readonly property int channelIndex: Number(modelData); readonly property string channelName: channelIndex < player.services.length ? player.services[channelIndex] : ""; width: ListView.view.width; height: 132; radius: 14; color: channelName === player.channelName ? "#26302a" : root.raised; border.color: channelName === player.channelName ? root.accent : "#24ffffff"
                        Column { anchors.fill: parent; anchors.margins: 14; spacing: 8
                            Item { width: parent.width; height: 32
                                Item { id: channelCardLogoBox; width: 56; height: 32; Image { id: channelCardLogo; anchors.fill: parent; source: channelIndex < player.channelLogoUrls.length ? player.channelLogoUrls[channelIndex] : ""; fillMode: Image.PreserveAspectFit; asynchronous: true } Label { anchors.centerIn: parent; visible: channelCardLogo.status !== Image.Ready; text: qsTr("局ロゴ"); color: root.muted; font.pixelSize: 9 } }
                                Label { anchors.left: channelCardLogoBox.right; anchors.leftMargin: 8; anchors.right: channelForce.left; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; text: channelName.replace(/^\d+\s+/, ""); color: root.ink; font.bold: true; elide: Text.ElideRight }
                                Label { id: channelForce; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: root.jikkyoForce(channelIndex); visible: text.length > 0; color: root.accent; font.pixelSize: 11; font.bold: true }
                            }
                            Label { width: parent.width; text: channelIndex < player.programTitles.length ? player.programTitles[channelIndex] : qsTr("番組情報なし"); color: root.ink; font.bold: true; elide: Text.ElideRight }
                            Label { text: root.programTime(channelIndex); color: root.muted; font.pixelSize: 11 }
                        }
                        Rectangle { anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.leftMargin: 14; anchors.rightMargin: 14; anchors.bottomMargin: 10; height: 3; radius: 2; color: "#32ffffff"
                            Rectangle { width: parent.width * root.programProgressAt(channelIndex); height: parent.height; radius: parent.radius; color: root.accent }
                        }
                        MouseArea { anchors.fill: parent; onClicked: player.selectChannel(channelIndex) }
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    z: 100
                    acceptedButtons: Qt.LeftButton
                    propagateComposedEvents: true
                    scrollGestureEnabled: false
                    onPressed: function(mouse) { mouse.accepted = false }
                    onClicked: function(mouse) { mouse.accepted = false }
                    onWheel: function(event) { root.scrollOneStep(sideChannelList, event, false, 144) }
                }
            }
            Item { visible: root.panel === "program"; Layout.fillHeight: true }
            Rectangle { Layout.fillWidth: true; height: 1; color: "#18ffffff" }
            RowLayout { Layout.fillWidth: true; spacing: 8
                Repeater { model: [["comments", "message-square", qsTr("コメント")], ["program", "info", qsTr("番組情報")], ["channels", "grid-2x2", qsTr("チャンネル")]]
                    Rectangle { required property var modelData; Layout.fillWidth: true; height: 54; radius: 12; color: root.panel === modelData[0] ? "#249caf9f" : "transparent"
                        Column { anchors.centerIn: parent; spacing: 3; Image { anchors.horizontalCenter: parent.horizontalCenter; width: 18; height: 18; source: root.uiIcon(modelData[1]); opacity: root.panel === modelData[0] ? 1 : .68 } Label { anchors.horizontalCenter: parent.horizontalCenter; text: modelData[2]; color: root.panel === modelData[0] ? root.ink : root.muted; font.pixelSize: 10 } }
                        MouseArea { anchors.fill: parent; onClicked: root.panel = modelData[0] }
                    }
                }
            }
        }
    }

    MouseArea {
        visible: root.channelsOpen
        anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top
        height: Math.max(0, root.height - channelPicker.height)
        z: 399
        cursorShape: Qt.PointingHandCursor
        onClicked: root.channelsOpen = false
    }

    Rectangle {
        id: channelPicker
        anchors.left: parent.left; anchors.right: parent.right; height: 304
        y: root.channelsOpen ? root.height - height : root.height
        visible: opacity > 0; enabled: root.channelsOpen; opacity: root.channelsOpen ? 1 : 0; color: "transparent"; z: 400
        Behavior on y { NumberAnimation { duration: 240; easing.type: Easing.OutCubic } }
        Behavior on opacity { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
        Rectangle { anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0; color: "#06000000" }
                GradientStop { position: .35; color: "#52000000" }
                GradientStop { position: 1; color: "#d6000000" }
            }
        }
        Column { anchors.fill: parent; anchors.leftMargin: 24; anchors.topMargin: 20; spacing: 14
            Row { width: parent.width - 24; spacing: 14
                RoundAction { iconSource: root.uiIcon("chevron-down"); tip: qsTr("折りたたむ"); onTriggered: root.channelsOpen = false }
                Label { text: qsTr("チャンネル"); color: root.ink; font.pixelSize: 22; font.bold: true; anchors.verticalCenter: parent.verticalCenter }
                Item { width: 24; height: 1 }
                BroadcastTabs { anchors.verticalCenter: parent.verticalCenter; value: root.channelPickerType; onSelected: function(channelType) { root.channelPickerType = channelType } }
            }
            Item { width: parent.width - 24; height: 190
                Flickable { id: channelPickerList; anchors.fill: parent; contentWidth: channelPickerRow.width; contentHeight: height; clip: true; boundsBehavior: Flickable.DragAndOvershootBounds; boundsMovement: Flickable.FollowBoundsBehavior
                    Row { id: channelPickerRow; spacing: 14
                        Repeater { model: player.services
                            delegate: Rectangle { required property int index; required property string modelData; readonly property bool matchesType: index < player.channelTypes.length && player.channelTypes[index] === root.channelPickerType; width: matchesType ? (modelData === player.channelName ? 356 : 270) : 0; height: 164; visible: matchesType; radius: 16; color: modelData === player.channelName ? "#26302a" : root.raised; border.color: modelData === player.channelName ? root.accent : "#30ffffff"
                                Column { anchors.fill: parent; anchors.margins: 14; spacing: 9
                                    Item { width: parent.width; height: 32
                                        Item { id: pickerLogoBox; width: 56; height: 32; Image { id: pickerLogo; anchors.fill: parent; source: index < player.channelLogoUrls.length ? player.channelLogoUrls[index] : ""; fillMode: Image.PreserveAspectFit; asynchronous: true } Label { anchors.centerIn: parent; visible: pickerLogo.status !== Image.Ready; text: qsTr("局ロゴ"); color: root.muted; font.pixelSize: 9 } }
                                        Label { anchors.left: pickerLogoBox.right; anchors.leftMargin: 8; anchors.right: pickerForce.left; anchors.rightMargin: 8; anchors.verticalCenter: parent.verticalCenter; text: modelData.replace(/^\d+\s+/, ""); color: root.muted; font.pixelSize: 12; elide: Text.ElideRight }
                                        Label { id: pickerForce; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: root.jikkyoForce(index); visible: text.length > 0; color: root.accent; font.pixelSize: 11; font.bold: true }
                                    }
                                    Label { width: parent.width; text: index < player.programTitles.length ? player.programTitles[index] : qsTr("番組情報なし"); color: root.ink; font.pixelSize: modelData === player.channelName ? 16 : 14; font.bold: true; wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight }
                                    Label { text: root.programTime(index); color: root.muted; font.pixelSize: 11 }
                                }
                                Rectangle { anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; anchors.leftMargin: 14; anchors.rightMargin: 14; anchors.bottomMargin: 8; height: 3; radius: 2; color: "#32ffffff"
                                    Rectangle { width: parent.width * root.programProgressAt(index); height: parent.height; radius: parent.radius; color: root.accent }
                                }
                                MouseArea { anchors.fill: parent; onClicked: { player.selectChannel(index); root.channelsOpen = false } }
                            }
                        }
                    }
                }
                MouseArea {
                    anchors.fill: parent
                    z: 100
                    acceptedButtons: Qt.LeftButton
                    propagateComposedEvents: true
                    scrollGestureEnabled: false
                    onPressed: function(mouse) { mouse.accepted = false }
                    onClicked: function(mouse) { mouse.accepted = false }
                    onWheel: function(event) { root.scrollOneStep(channelPickerList, event, true, 112) }
                }
            }
        }
    }

    Rectangle {
        id: guide; anchors.fill: parent; visible: root.guideOpen; color: "#0b0c0b"; z: 500
        readonly property real pixelsPerMinute: 2.4
        readonly property real channelWidth: 222
        function selectDay(index) {
            const next = Math.max(0, Math.min(6, index))
            if (next === root.guideDayOffset) return
            root.guideDayOffset = next
            root.selectedGuideIndex = -1
            if (guideDateGroup.visible) guideDateGroup.revealSelected()
            if (next === 0) Qt.callLater(root.scrollGuideToNow)
            else epgFlick.contentY = 0
        }
        function scrollHorizontally(event) {
            const delta = event.angleDelta.y || event.angleDelta.x
            if (delta === 0) return
            epgFlick.cancelFlick()
            epgHorizontalScroll.stop()
            const maximum = Math.max(0, epgFlick.contentWidth - epgFlick.width)
            epgHorizontalScroll.from = epgFlick.contentX
            epgHorizontalScroll.to = Math.max(0, Math.min(maximum,
                epgFlick.contentX + (delta < 0 ? channelWidth : -channelWidth)))
            epgHorizontalScroll.start()
            event.accepted = true
        }
        NumberAnimation { id: epgHorizontalScroll; target: epgFlick; property: "contentX"; duration: 150; easing.type: Easing.OutCubic }
        readonly property real dayStart: {
            const date = new Date(root.nowMs)
            date.setHours(0, 0, 0, 0)
            return date.getTime() + root.guideDayOffset * 86400000
        }
        readonly property var programsByChannel: {
            const result = []
            for (let i = 0; i < player.services.length; ++i) result.push([])
            const starts = player.guideStarts
            const durations = player.guideDurations
            const channels = player.guideChannelIndices
            for (let i = 0; i < starts.length; ++i) {
                const channelIndex = Number(channels[i])
                const start = Number(starts[i])
                const duration = Number(durations[i])
                if (start < dayStart + 86400000
                        && start + duration > dayStart)
                    result[channelIndex].push(i)
            }
            return result
        }
        Rectangle { id: guideToolbar; anchors.left: parent.left; anchors.right: parent.right; anchors.top: parent.top; height: 84; color: "#151715"
            MouseArea { anchors.fill: parent; z: 0; acceptedButtons: Qt.LeftButton; onPressed: root.startSystemMove(); onDoubleClicked: root.visibility === Window.Maximized ? root.showNormal() : root.showMaximized() }
            WindowButtons { id: guideWindowButtons; anchors.right: parent.right; anchors.top: parent.top; anchors.rightMargin: 18; anchors.topMargin: 18; z: 1 }
            RowLayout { anchors.left: parent.left; anchors.right: guideWindowButtons.left; anchors.top: parent.top; anchors.leftMargin: 18; anchors.rightMargin: 14; anchors.topMargin: 18; height: 42; z: 1; spacing: root.width < 980 ? 8 : 14
                RoundAction { iconSource: root.uiIcon("chevron-left"); onTriggered: { root.selectedGuideIndex = -1; root.guideOpen = false } }
                Label { visible: root.width >= 900; text: qsTr("番組表"); color: root.ink; font.pixelSize: 26; font.bold: true }
                BroadcastTabs { value: root.guideType; onSelected: function(channelType) { root.guideType = channelType; root.selectedGuideIndex = -1; epgFlick.contentX = 0 } }
                Rectangle { id: guideDateCompact; visible: root.width < 1280; Layout.preferredWidth: 202; implicitHeight: 40; radius: 20; color: "#b8171918"; border.color: "#32ffffff"
                    Connections { target: root; function onGuideDayOffsetChanged() { compactDateChange.restart() } }
                    SequentialAnimation { id: compactDateChange
                        NumberAnimation { target: compactDateLabel; property: "opacity"; to: .35; duration: 70; easing.type: Easing.InCubic }
                        NumberAnimation { target: compactDateLabel; property: "opacity"; to: 1; duration: 120; easing.type: Easing.OutCubic }
                    }
                    Rectangle { anchors.left: parent.left; anchors.top: parent.top; anchors.bottom: parent.bottom; width: 44; radius: height / 2; color: compactPrevious.containsMouse && root.guideDayOffset > 0 ? "#28ffffff" : "transparent"; opacity: root.guideDayOffset > 0 ? 1 : .35
                        Image { anchors.centerIn: parent; width: 16; height: 16; source: root.uiIcon("chevron-left") }
                        MouseArea { id: compactPrevious; anchors.fill: parent; enabled: root.guideDayOffset > 0; hoverEnabled: true; cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: guide.selectDay(root.guideDayOffset - 1) }
                    }
                    Label { id: compactDateLabel; anchors.centerIn: parent; text: root.guideDayOffset === 0 ? qsTr("今日") : Qt.formatDate(new Date(guide.dayStart), "M/d（ddd）"); color: root.ink; font.pixelSize: 12; font.bold: true }
                    Rectangle { anchors.right: parent.right; anchors.top: parent.top; anchors.bottom: parent.bottom; width: 44; radius: height / 2; color: compactNext.containsMouse && root.guideDayOffset < 6 ? "#28ffffff" : "transparent"; opacity: root.guideDayOffset < 6 ? 1 : .35
                        Image { anchors.centerIn: parent; width: 16; height: 16; source: root.uiIcon("chevron-left"); mirror: true }
                        MouseArea { id: compactNext; anchors.fill: parent; enabled: root.guideDayOffset < 6; hoverEnabled: true; cursorShape: enabled ? Qt.PointingHandCursor : Qt.ArrowCursor; onClicked: guide.selectDay(root.guideDayOffset + 1) }
                    }
                }
                Rectangle { id: guideDateGroup; visible: root.width >= 1280; Layout.fillWidth: visible; Layout.minimumWidth: visible ? 82 : 0; Layout.maximumWidth: 572; implicitHeight: 40; radius: 20; color: "#b8171918"; border.color: "#32ffffff"; clip: true
                    function itemX(index) { return 3 + (index === 0 ? 0 : 62 + (index - 1) * 84) }
                    function itemWidth(index) { return index === 0 ? 62 : 84 }
                    function revealSelected() {
                        const center = itemX(root.guideDayOffset) + itemWidth(root.guideDayOffset) / 2
                        const maximum = Math.max(0, guideDateFlick.contentWidth - guideDateFlick.width)
                        guideDateScroll.to = Math.max(0, Math.min(maximum, center - guideDateFlick.width / 2))
                        guideDateScroll.restart()
                    }
                    Flickable { id: guideDateFlick; anchors.fill: parent; contentWidth: 3 + guideDateRow.width + 3; contentHeight: height; boundsBehavior: Flickable.StopAtBounds; flickableDirection: Flickable.HorizontalFlick
                        NumberAnimation { id: guideDateScroll; target: guideDateFlick; property: "contentX"; duration: 170; easing.type: Easing.OutCubic }
                        Rectangle { x: guideDateGroup.itemX(root.guideDayOffset); y: 3; width: guideDateGroup.itemWidth(root.guideDayOffset); height: parent.height - 6; radius: height / 2; color: "#429caf9f"; border.color: root.accent
                            Behavior on x { NumberAnimation { duration: 170; easing.type: Easing.OutCubic } }
                            Behavior on width { NumberAnimation { duration: 170; easing.type: Easing.OutCubic } }
                        }
                        Row { id: guideDateRow; x: 3; width: 62 + 6 * 84; height: parent.height
                            Repeater { model: 7
                                Item { required property int index; width: guideDateGroup.itemWidth(index); height: guideDateRow.height
                                    Label { anchors.centerIn: parent; text: index === 0 ? qsTr("今日") : Qt.formatDate(new Date(guide.dayStart - root.guideDayOffset * 86400000 + index * 86400000), "M/d（ddd）"); color: root.guideDayOffset === index ? root.ink : "#d5d8d5"; font.pixelSize: 12; font.bold: root.guideDayOffset === index }
                                    MouseArea { anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: guide.selectDay(index) }
                                }
                            }
                        }
                    }
                }
                Item { Layout.fillWidth: true }
                RoundAction { iconSource: root.uiIcon("settings-2"); onTriggered: settings.open() }
            }
        }
        Item { id: guideBody; anchors.left: parent.left; anchors.right: parent.right; anchors.top: guideToolbar.bottom; anchors.bottom: guideFooter.top
            Rectangle { anchors.left: parent.left; anchors.top: parent.top; width: 104; height: 88; color: "#111311" }
            Item { anchors.left: parent.left; anchors.top: parent.top; anchors.bottom: parent.bottom; width: 104; z: 4; clip: true
                Repeater { model: 25
                    Label { required property int index; x: 24; y: 80 + index * 60 * guide.pixelsPerMinute - 8 - epgFlick.contentY; text: root.guideClock(guide.dayStart + index * 3600000); color: root.muted; font.pixelSize: 12; font.bold: true }
                }
                Rectangle { visible: root.nowMs >= guide.dayStart && root.nowMs < guide.dayStart + 86400000; x: 20; y: 88 + (root.nowMs - guide.dayStart) / 60000 * guide.pixelsPerMinute - 12 - epgFlick.contentY; width: 68; height: 24; radius: 12; color: root.accent
                    Label { anchors.centerIn: parent; text: root.guideClock(root.nowMs); color: "#17201a"; font.pixelSize: 11; font.bold: true }
                }
            }
            Flickable { id: epgFlick; x: 104; width: parent.width - 104; height: parent.height; clip: true; contentWidth: Math.max(width, root.guideChannelCount() * guide.channelWidth); contentHeight: 88 + 1440 * guide.pixelsPerMinute; boundsBehavior: Flickable.StopAtBounds
                Item { width: epgFlick.contentWidth; height: epgFlick.contentHeight
                    Repeater { model: player.services
                        Rectangle { required property int index; required property string modelData; visible: root.guideChannelVisible(index); z: 30; x: root.guideColumn(index) * guide.channelWidth; y: epgFlick.contentY; width: guide.channelWidth - 4; height: 88; color: "#151715"
                            Row { anchors.left: parent.left; anchors.leftMargin: 14; anchors.verticalCenter: parent.verticalCenter; spacing: 8
                                Item { width: 56; height: 32
                                    Image { id: epgLogo; anchors.fill: parent; source: index < player.channelLogoUrls.length ? player.channelLogoUrls[index] : ""; fillMode: Image.PreserveAspectFit; asynchronous: true }
                                    Label { anchors.centerIn: parent; visible: epgLogo.status !== Image.Ready; text: qsTr("局ロゴ"); color: root.muted; font.pixelSize: 10 }
                                }
                                Label { width: guide.channelWidth - 94; anchors.verticalCenter: parent.verticalCenter; text: modelData.replace(/^\d+\s+/, ""); color: root.ink; font.bold: true; elide: Text.ElideRight }
                            }
                        }
                    }
                    Repeater { model: 25
                        Rectangle { required property int index; x: 0; y: 88 + index * 60 * guide.pixelsPerMinute; width: epgFlick.contentWidth; height: 1; color: "#20ffffff" }
                    }
                    Repeater { model: player.services
                        Loader { id: channelPrograms; required property int index
                            readonly property real columnX: root.guideColumn(index) * guide.channelWidth
                            x: columnX; y: 88; width: guide.channelWidth - 4; height: epgFlick.contentHeight - 88
                            active: root.guideChannelVisible(index)
                                && columnX + width >= epgFlick.contentX - guide.channelWidth
                                && columnX <= epgFlick.contentX + epgFlick.width + guide.channelWidth
                            sourceComponent: Component {
                                Item { width: channelPrograms.width; height: channelPrograms.height
                                    Repeater { model: guide.programsByChannel[channelPrograms.index] || []
                                        Rectangle { required property var modelData
                                            readonly property int programIndex: Number(modelData)
                                            readonly property real startValue: Number(player.guideStarts[programIndex])
                                            readonly property real durationValue: Number(player.guideDurations[programIndex])
                                            y: Math.max(0, (startValue - guide.dayStart) / 60000 * guide.pixelsPerMinute)
                                            width: channelPrograms.width; height: Math.max(24, durationValue / 60000 * guide.pixelsPerMinute - 4)
                                            color: root.guideColor(player.guideGenres[programIndex]); radius: 0
                                            border.width: root.selectedGuideIndex === programIndex ? 4 : 1
                                            border.color: root.selectedGuideIndex === programIndex ? root.accent : "#5b625e"
                                            Column { anchors.fill: parent; anchors.margins: 10; spacing: 5
                                                Label { width: parent.width; text: player.guideTitles[programIndex]; color: "#1b201d"; font.pixelSize: 13; font.bold: true; wrapMode: Text.Wrap; maximumLineCount: Math.max(1, Math.floor((parent.height - 22) / 17)); elide: Text.ElideRight }
                                                Label { visible: parent.height > 46; text: root.guideClock(startValue) + "–" + root.guideClock(startValue + durationValue); color: "#4e5651"; font.pixelSize: 10 }
                                            }
                                            MouseArea { anchors.fill: parent; onClicked: guideDetail.openFor(programIndex) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Rectangle { visible: root.nowMs >= guide.dayStart && root.nowMs < guide.dayStart + 86400000; z: 12; x: 0; y: 88 + (root.nowMs - guide.dayStart) / 60000 * guide.pixelsPerMinute; width: epgFlick.contentWidth; height: 2; color: root.accent }
                }
            }
            MouseArea {
                x: epgFlick.x; y: epgFlick.y; width: epgFlick.width; height: epgFlick.height
                z: 18
                acceptedButtons: Qt.LeftButton
                propagateComposedEvents: true
                scrollGestureEnabled: false
                onPressed: function(mouse) { mouse.accepted = false }
                onClicked: function(mouse) { mouse.accepted = false }
                onWheel: function(event) {
                    if (event.modifiers & Qt.ShiftModifier)
                        guide.scrollHorizontally(event)
                    else
                        event.accepted = false
                }
            }
            MouseArea { anchors.fill: parent; visible: root.selectedGuideIndex >= 0; z: 19; onClicked: root.selectedGuideIndex = -1 }
            Rectangle { id: guideDetail; visible: opacity > 0; enabled: root.selectedGuideIndex >= 0; opacity: root.selectedGuideIndex >= 0 ? 1 : 0; scale: root.selectedGuideIndex >= 0 ? 1 : .97; z: 20; width: Math.min(500, parent.width - 48); height: 360; radius: 18; color: "#151715"; border.color: "#b8ffffff"
                readonly property bool selectedProgramIsLive: root.selectedGuideIndex >= 0
                    && Number(player.guideStarts[root.selectedGuideIndex]) <= root.nowMs
                    && root.nowMs < Number(player.guideStarts[root.selectedGuideIndex]) + Number(player.guideDurations[root.selectedGuideIndex])
                function openFor(index) {
                    const cellX = 104 + root.guideColumn(Number(player.guideChannelIndices[index])) * guide.channelWidth - epgFlick.contentX
                    x = cellX > parent.width / 2
                        ? Math.max(24, cellX - width - 28)
                        : Math.min(parent.width - width - 24, cellX + guide.channelWidth + 24)
                    y = Math.max(20, Math.min(parent.height - height - 20,
                        88 + (Number(player.guideStarts[index]) - guide.dayStart) / 60000 * guide.pixelsPerMinute - epgFlick.contentY))
                    root.selectedGuideIndex = index
                }
                transformOrigin: Item.Center
                Behavior on opacity { NumberAnimation { duration: 150; easing.type: Easing.OutCubic } }
                Behavior on scale { NumberAnimation { duration: 180; easing.type: Easing.OutBack } }
                x: 24; y: 20
                MouseArea { anchors.fill: parent }
                Column { anchors.fill: parent; anchors.margins: 28; spacing: 14
                    Label { text: root.selectedGuideIndex >= 0 ? root.guideClock(player.guideStarts[root.selectedGuideIndex]) + "–" + root.guideClock(Number(player.guideStarts[root.selectedGuideIndex]) + Number(player.guideDurations[root.selectedGuideIndex])) : ""; color: root.accent; font.bold: true }
                    Label { width: parent.width; text: root.selectedGuideIndex >= 0 ? player.guideTitles[root.selectedGuideIndex] : ""; color: root.ink; font.pixelSize: 22; font.bold: true; wrapMode: Text.Wrap; maximumLineCount: 3; elide: Text.ElideRight }
                    Label { width: parent.width; text: root.selectedGuideIndex >= 0 ? player.services[Number(player.guideChannelIndices[root.selectedGuideIndex])] : ""; color: root.muted }
                    Rectangle { width: parent.width; height: 1; color: "#20ffffff" }
                    Label { width: parent.width; height: 88; text: root.selectedGuideIndex >= 0 ? player.guideDescriptions[root.selectedGuideIndex] : ""; color: "#d9dcda"; wrapMode: Text.Wrap; elide: Text.ElideRight }
                    Item { width: 1; height: 4 }
                    Row { spacing: 16
                        Rectangle { visible: guideDetail.selectedProgramIsLive; width: visible ? 168 : 0; height: 44; radius: 22; color: root.accent; Label { anchors.centerIn: parent; text: qsTr("この番組を視聴"); color: "#17201a"; font.bold: true } MouseArea { anchors.fill: parent; onClicked: { player.selectChannel(Number(player.guideChannelIndices[root.selectedGuideIndex])); root.selectedGuideIndex = -1; root.guideOpen = false } } }
                    }
                }
            }
        }
        Rectangle { id: guideFooter; anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom; height: 60; color: "#0b0c0b"; border.color: "#18ffffff"; Label { anchors.left: parent.left; anchors.leftMargin: 24; anchors.verticalCenter: parent.verticalCenter; text: qsTr("←→ チャンネル移動　 ↑↓ 時間移動　 Enter 詳細"); color: root.muted; font.pixelSize: 12 } }
    }

    Popup {
        id: playbackSettings
        parent: Overlay.overlay
        width: 320; height: 300
        x: Math.max(20, videoRegion.width - width - 24)
        y: Math.max(20, root.height - height - 92)
        modal: false; dim: false; padding: 20
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        onClosed: { player.saveSettings(); root.reveal() }
        background: Rectangle { radius: 18; color: "#f21a1c1a"; border.color: "#42ffffff" }
        contentItem: ColumnLayout {
            spacing: 8
            Label { text: qsTr("再生設定"); color: root.ink; font.pixelSize: 17; font.bold: true }
            RowLayout { Layout.fillWidth: true
                Label { text: qsTr("弾幕コメント"); color: root.ink; font.pixelSize: 13 }
                Item { Layout.fillWidth: true }
                ToggleSwitch { checked: root.danmaku; onToggled: root.danmaku = !root.danmaku }
            }
            RowLayout { Layout.fillWidth: true; Label { text: qsTr("文字サイズ"); color: root.muted; font.pixelSize: 11 } Item { Layout.fillWidth: true } Label { text: Math.round(root.commentFontSize) + " px"; color: root.ink; font.pixelSize: 11 } }
            Slider { Layout.fillWidth: true; from: 14; to: 36; stepSize: 1; value: root.commentFontSize; onMoved: root.commentFontSize = value }
            RowLayout { Layout.fillWidth: true; Label { text: qsTr("不透明度"); color: root.muted; font.pixelSize: 11 } Item { Layout.fillWidth: true } Label { text: Math.round(root.commentOpacity * 100) + "%"; color: root.ink; font.pixelSize: 11 } }
            Slider { Layout.fillWidth: true; from: .2; to: 1; stepSize: .05; value: root.commentOpacity; onMoved: root.commentOpacity = value }
            RowLayout { Layout.fillWidth: true; Label { text: qsTr("速度"); color: root.muted; font.pixelSize: 11 } Item { Layout.fillWidth: true } Label { text: root.commentSpeed.toFixed(1) + "×"; color: root.ink; font.pixelSize: 11 } }
            Slider { Layout.fillWidth: true; from: .5; to: 2; stepSize: .1; value: root.commentSpeed; onMoved: root.commentSpeed = value }
        }
    }

    Drawer {
        id: settings; edge: Qt.RightEdge; width: Math.min(420, root.width * .88); height: root.height; modal: true; dim: true; onOpened: root.reveal()
        background: Rectangle { color: "#fc151715"; border.color: "#28ffffff" }
        ColumnLayout { anchors.fill: parent; anchors.margins: 28; spacing: 18
            RowLayout { Layout.fillWidth: true; Label { text: qsTr("接続設定"); color: root.ink; font.pixelSize: 23; font.bold: true } Item { Layout.fillWidth: true } RoundAction { iconSource: root.uiIcon("panel-right-close"); tip: qsTr("折りたたむ"); onTriggered: settings.close() } }
            Label { text: qsTr("Mirakurunサーバー"); color: root.muted }
            TextField { id: serverField; Layout.fillWidth: true; height: 48; placeholderText: "http://mirakurun:40772"; text: player.server; color: root.ink; placeholderTextColor: "#8c918c"; leftPadding: 16; rightPadding: 16; background: Rectangle { radius: 12; color: root.raised; border.color: serverField.activeFocus ? root.accent : "#30ffffff" } }
            Label { text: qsTr("サービスID"); color: root.muted }
            TextField { id: serviceField; Layout.fillWidth: true; height: 48; placeholderText: "3203246080"; text: player.serviceId; inputMethodHints: Qt.ImhDigitsOnly; color: root.ink; placeholderTextColor: "#8c918c"; leftPadding: 16; rightPadding: 16; background: Rectangle { radius: 12; color: root.raised; border.color: serviceField.activeFocus ? root.accent : "#30ffffff" } }
            Rectangle { Layout.fillWidth: true; height: 46; radius: 23; color: root.accent; Label { anchors.centerIn: parent; text: qsTr("適用して視聴"); color: "#17201a"; font.bold: true } MouseArea { anchors.fill: parent; onClicked: { player.server = serverField.text; player.serviceId = serviceField.text; player.saveSettings(); player.play(); settings.close() } } }
            Item { Layout.fillHeight: true }
            Label { text: "F11  " + qsTr("全画面") + "　 Space  " + qsTr("一時停止"); color: "#929497" }
        }
    }

    component ResizeEdge: MouseArea { required property int edges; enabled: root.visibility === Window.Windowed; acceptedButtons: Qt.LeftButton; z: 1000; onPressed: root.startSystemResize(edges) }
    ResizeEdge { edges: Qt.LeftEdge; anchors { left: parent.left; top: parent.top; bottom: parent.bottom } width: 10; cursorShape: Qt.SizeHorCursor }
    ResizeEdge { edges: Qt.RightEdge; anchors { right: parent.right; top: parent.top; bottom: parent.bottom } width: 10; cursorShape: Qt.SizeHorCursor }
    ResizeEdge { edges: Qt.TopEdge; anchors { left: parent.left; right: parent.right; top: parent.top } height: 10; cursorShape: Qt.SizeVerCursor }
    ResizeEdge { edges: Qt.BottomEdge; anchors { left: parent.left; right: parent.right; bottom: parent.bottom } height: 10; cursorShape: Qt.SizeVerCursor }
    ResizeEdge { edges: Qt.LeftEdge | Qt.TopEdge; anchors { left: parent.left; top: parent.top } width: 18; height: 18; z: 1001; cursorShape: Qt.SizeFDiagCursor }
    ResizeEdge { edges: Qt.RightEdge | Qt.TopEdge; anchors { right: parent.right; top: parent.top } width: 18; height: 18; z: 1001; cursorShape: Qt.SizeBDiagCursor }
    ResizeEdge { edges: Qt.LeftEdge | Qt.BottomEdge; anchors { left: parent.left; bottom: parent.bottom } width: 18; height: 18; z: 1001; cursorShape: Qt.SizeBDiagCursor }
    ResizeEdge { edges: Qt.RightEdge | Qt.BottomEdge; anchors { right: parent.right; bottom: parent.bottom } width: 18; height: 18; z: 1001; cursorShape: Qt.SizeFDiagCursor }

    Component.onCompleted: {
        videoAttached = player.attachVideoItem(videoItem)
        if (!videoAttached) return
        root.refreshChannelsIfDue(true); hideTimer.start()
    }
}
