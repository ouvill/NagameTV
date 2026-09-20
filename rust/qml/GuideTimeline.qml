pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Item {
    id: root
    enum ProgramLayout { NoText, OneLine, Full }
    required property var rows
    required property string programsJson
    required property double dayStart
    required property double dayEnd
    property int openingIndex: -1
    property var selectedProgram: null
    signal selected(var program, point cellPosition, string channelLabel)
    readonly property real timeRulerWidth: 36
    readonly property real channelHeaderHeight: 40
    readonly property real channelHeaderPadding: 5
    readonly property real channelHeaderSpacing: 6
    readonly property real channelLogoWidth: 52
    readonly property real minimumChannelWidth: 172
    readonly property int visibleChannelCount: Math.max(1, Math.min(rows.length,
        Math.floor(timelineView.width / minimumChannelWidth)))
    readonly property real channelWidth: Math.max(minimumChannelWidth, timelineView.width / visibleChannelCount)
    readonly property real dividerWidth: 1
    readonly property color dividerColor: "#e4e7e4"
    readonly property color headerColor: "#202721"
    readonly property color headerDividerColor: "#465049"
    readonly property color timeGridColor: "#39453d"
    readonly property color currentTimeColor: "#b44b3f"
    readonly property real cellPadding: 8
    readonly property real cellTopPadding: 4
    readonly property real textSpacing: 2
    readonly property real titleLineHeight: 18
    readonly property real compactTitleLineHeight: 14
    readonly property real timeLabelHeight: 18
    readonly property real minimumFullCellHeight: cellTopPadding + timeLabelHeight + textSpacing + titleLineHeight + textSpacing
    readonly property real pixelsPerMinute: 3
    readonly property int minutesPerHour: 60
    readonly property int timeTickMinutes: 30
    readonly property int timeBandHours: 3
    readonly property var timeBandColors: ["#00337f", "#00667f", "#007f66", "#667f00", "#7f6600", "#7f3300", "#7f0066", "#66007f"]
    readonly property var columns: JSON.parse(programsJson)
    property double now: Date.now()
    readonly property bool today: now >= dayStart && now < dayEnd
    readonly property real currentTimeY: channelHeaderHeight + (now - dayStart) / 60000 * pixelsPerMinute
    // Boundary bindings can update separately during a date change.
    readonly property double dayDuration: Math.max(0, dayEnd - dayStart)
    readonly property int hours: Math.ceil(dayDuration / 3600000)
    function timeBandColor(time) {
        // Use local clock hours, including repeated/skipped hours on DST days.
        return timeBandColors[Math.floor(new Date(time).getHours() / timeBandHours)]
    }
    // Keep only identity and position; delegates can be unloaded offscreen and
    // the EPG snapshot can be replaced while keyboard navigation is active.
    property int cursorColumn: 0
    property string cursorKey: ""
    // null means navigation has not started. An empty channel or a replaced
    // program can have no cursorProgram while still retaining a chosen time.
    property var cursorTime: null
    readonly property var cursorProgram: cursorColumn < rows.length && cursorKey.length
        ? schedule(rows[cursorColumn].index).find(program => program.watchKey === cursorKey) || null : null
    function chooseProgram(time) {
        cursorTime = time
        if (cursorColumn >= rows.length) return
        const programs = schedule(rows[cursorColumn].index)
        let closest = null
        let distance = Number.POSITIVE_INFINITY
        for (const program of programs) {
            const end = program.startAt + program.duration
            const candidate = time < program.startAt ? program.startAt - time : time >= end ? time - end + 1 : 0
            if (candidate < distance) { closest = program; distance = candidate }
        }
        cursorKey = closest ? closest.watchKey : ""
    }
    function revealCursor() {
        wheelInput.cancelGesture()
        horizontalScroll.stop()
        timelineView.cancelFlick()
        const left = cursorColumn * channelWidth
        timelineView.contentX = Math.max(0, Math.min(Math.max(0, timelineView.contentWidth - timelineView.width),
            left < timelineView.contentX ? left : Math.max(timelineView.contentX, left + channelWidth - timelineView.width)))
        if (!cursorProgram) return
        const top = channelHeaderHeight + (Math.max(dayStart, cursorProgram.startAt) - dayStart) / 60000 * pixelsPerMinute
        // Align the start of long programs below the sticky channel header.
        if (top < timelineView.contentY + root.channelHeaderHeight || top + 24 > timelineView.contentY + timelineView.height)
            timelineView.contentY = Math.max(0, Math.min(Math.max(0, timelineView.contentHeight - timelineView.height), top - channelHeaderHeight))
    }
    function navigate(key) {
        if (!rows.length) return
        if (!cursorProgram) chooseProgram(cursorTime === null ? (today ? now : dayStart) : cursorTime)
        if (key === Qt.Key_Left || key === Qt.Key_Right) {
            const time = cursorTime
            cursorColumn = Math.max(0, Math.min(rows.length - 1, cursorColumn + (key === Qt.Key_Left ? -1 : 1)))
            chooseProgram(time)
        } else if (key === Qt.Key_Up || key === Qt.Key_Down) {
            const programs = schedule(rows[cursorColumn].index)
            const index = programs.findIndex(program => program.watchKey === cursorKey)
            // Rust projects programs in start-time order; do not sort/copy them per key.
            if (index >= 0) {
                const program = programs[Math.max(0, Math.min(programs.length - 1, index + (key === Qt.Key_Up ? -1 : 1)))]
                cursorKey = program.watchKey
                cursorTime = Math.max(dayStart, program.startAt)
            }
        }
        revealCursor()
        if ((key === Qt.Key_Return || key === Qt.Key_Enter) && cursorProgram) {
            const top = channelHeaderHeight + (Math.max(dayStart, cursorProgram.startAt) - dayStart) / 60000 * pixelsPerMinute
            selected(cursorProgram, Qt.point(timelineView.x + cursorColumn * channelWidth - timelineView.contentX, top - timelineView.contentY), rows[cursorColumn].label)
        }
    }
    Keys.onPressed: function(event) {
        if ((event.modifiers & ~Qt.KeypadModifier) !== Qt.NoModifier || selectedProgram) return
        if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down, Qt.Key_Return, Qt.Key_Enter].includes(event.key)) {
            navigate(event.key)
            event.accepted = true
        }
    }
    function genreColor(genre) {
        const colors = ["#ffffe0", "#e0e0ff", "#ffe0f0", "#ffe0e0", "#e0ffe0", "#e0ffff", "#fff0e0", "#ffe0ff", "#ffffe0", "#fff0e0", "#e0f0ff", "#e0f0ff"]
        return Number.isInteger(genre) && genre >= 0 && genre < colors.length ? colors[genre] : "#f0f0f0"
    }
    function schedule(index) {
        const column = columns.find(column => column.index === index)
        return column ? column.programs : []
    }
    function scrollHorizontally(event) {
        const delta = event.angleDelta.y || event.angleDelta.x
        if (!delta) return
        wheelInput.cancelGesture()
        timelineView.cancelFlick()
        horizontalScroll.stop()
        horizontalScroll.from = timelineView.contentX
        horizontalScroll.to = Math.max(0, Math.min(Math.max(0, timelineView.contentWidth - timelineView.width),
            timelineView.contentX + (delta < 0 ? channelWidth : -channelWidth)))
        horizontalScroll.start()
        event.accepted = true
    }
    NumberAnimation { id: horizontalScroll; target: timelineView; property: "contentX"; duration: 150; easing.type: Easing.OutCubic }
    function resetPosition() {
        wheelInput.cancelGesture()
        horizontalScroll.stop()
        timelineView.cancelFlick()
        timelineView.contentY = now >= dayStart && now < dayEnd
            ? Math.max(0, Math.min(timelineView.contentHeight - timelineView.height, channelHeaderHeight + (now - dayStart) / 60000 * pixelsPerMinute - timelineView.height * 0.34)) : 0
    }
    function revealOpeningChannel() {
        horizontalScroll.stop()
        wheelInput.cancelGesture()
        timelineView.cancelFlick()
        const index = rows.findIndex(row => row.index === openingIndex)
        cursorColumn = index >= 0 ? index : 0
        cursorKey = ""
        cursorTime = null
        timelineView.contentX = Math.max(0, Math.min(timelineView.contentWidth - timelineView.width, cursorColumn * channelWidth))
    }
    onDayStartChanged: { cursorKey = ""; cursorTime = null; Qt.callLater(resetPosition) }
    onRowsChanged: {
        cursorColumn = 0
        cursorKey = ""
        cursorTime = null
        Qt.callLater(revealOpeningChannel)
    }
    Component.onCompleted: resetPosition()
    Timer { interval: 1000; repeat: true; running: root.visible; onTriggered: root.now = Date.now() }
    Item {
        width: root.timeRulerWidth; height: parent.height; clip: true
        Repeater {
            model: root.hours
            Rectangle {
                required property int index
                objectName: "guideTimeBand" + index
                width: root.timeRulerWidth
                height: root.minutesPerHour * root.pixelsPerMinute
                y: root.channelHeaderHeight + index * height - timelineView.contentY
                color: root.timeBandColor(root.dayStart + index * 3600000)
            }
        }
        Repeater {
            model: Math.ceil(root.dayDuration / (root.timeTickMinutes * 60000))
            Item {
                id: timeTick
                required property int index
                readonly property double time: root.dayStart + index * root.timeTickMinutes * 60000
                width: root.timeRulerWidth; height: root.timeTickMinutes * root.pixelsPerMinute
                y: root.channelHeaderHeight + index * height - timelineView.contentY
                Rectangle { width: parent.width; height: root.dividerWidth; color: root.timeGridColor }
                Label {
                    width: parent.width; y: root.cellTopPadding
                    text: Qt.formatTime(new Date(timeTick.time), "hh\nmm")
                    horizontalAlignment: Text.AlignHCenter
                    color: "#ffffff"
                    font.pixelSize: timeTick.index % 2 === 0 ? 12 : 10
                    font.bold: timeTick.index % 2 === 0
                    lineHeightMode: Text.FixedHeight; lineHeight: 13
                }
            }
        }
        Rectangle {
            objectName: "guideCurrentTimeBadge"
            visible: root.today && root.currentTimeY - timelineView.contentY >= root.channelHeaderHeight
            y: root.currentTimeY - height / 2 - timelineView.contentY
            width: root.timeRulerWidth; height: 34; color: root.currentTimeColor
            Label {
                anchors.centerIn: parent
                text: Qt.formatTime(new Date(root.now), "hh\nmm")
                horizontalAlignment: Text.AlignHCenter
                color: "#ffffff"; font.pixelSize: 11; font.bold: true
            }
        }
        Rectangle {
            width: parent.width; height: root.channelHeaderHeight
            color: root.headerColor
        }
    }
    Flickable {
        id: timelineView
        objectName: "guideTimeline"
        x: root.timeRulerWidth; width: Math.max(0, parent.width - root.timeRulerWidth); height: parent.height; clip: true
        contentWidth: Math.max(width, root.rows.length * root.channelWidth)
        contentHeight: root.channelHeaderHeight + root.dayDuration / 60000 * root.pixelsPerMinute
        boundsBehavior: Flickable.StopAtBounds
        maximumFlickVelocity: wheelInput.maximumFlickSpeedPixelsPerSecond
        flickDeceleration: wheelInput.flickDecelerationPixelsPerSecondSquared
        onDraggingChanged: if (dragging) { wheelInput.cancelGesture(); horizontalScroll.stop() }
        Repeater {
            model: Math.ceil(root.dayDuration / (root.timeTickMinutes * 60000)) + 1
            Rectangle {
                required property int index
                y: root.channelHeaderHeight + index * root.timeTickMinutes * root.pixelsPerMinute
                width: timelineView.contentWidth; height: root.dividerWidth; color: root.timeGridColor
            }
        }
        Repeater {
            model: root.rows
            Loader {
                id: column
                required property int index
                required property var modelData
                objectName: "guideColumn" + index
                x: index * root.channelWidth
                width: root.channelWidth; height: timelineView.contentHeight
                active: x + width >= timelineView.contentX - root.channelWidth
                    && x <= timelineView.contentX + timelineView.width + root.channelWidth
                sourceComponent: Rectangle {
                    width: column.width; height: column.height
                    color: root.dividerColor
                    Repeater {
                        model: root.schedule(column.modelData.index)
                        Rectangle {
                            id: cell
                            required property var modelData
                            objectName: "guideCell"
                            readonly property double begin: Math.max(root.dayStart, modelData.startAt)
                            readonly property double end: Math.min(root.dayEnd, modelData.startAt + modelData.duration)
                            readonly property int layoutMode: height < root.compactTitleLineHeight ? GuideTimeline.NoText
                                : height < root.minimumFullCellHeight ? GuideTimeline.OneLine : GuideTimeline.Full
                            readonly property string startLabel: Qt.formatTime(new Date(modelData.startAt), "hh:mm")
                            readonly property string title: modelData.name || qsTranslate("Viewer", "No program information")
                            y: root.channelHeaderHeight + (begin - root.dayStart) / 60000 * root.pixelsPerMinute
                            x: root.dividerWidth
                            width: column.width - root.dividerWidth * 2
                            height: Math.max(1, (end - begin) / 60000 * root.pixelsPerMinute - root.dividerWidth)
                            clip: true
                            color: root.genreColor(modelData.genre)
                            readonly property bool highlighted: modelData === root.selectedProgram
                                || (root.activeFocus && column.index === root.cursorColumn && root.cursorKey.length > 0 && modelData.watchKey === root.cursorKey)
                            border.width: highlighted ? 2 : 0
                            border.color: "#9caf9f"
                            Label {
                                objectName: "guideShortProgram"
                                visible: cell.layoutMode === GuideTimeline.OneLine
                                x: root.cellPadding - root.textSpacing
                                width: parent.width - x * 2; height: parent.height
                                text: cell.startLabel + " " + cell.title
                                textFormat: Text.PlainText; elide: Text.ElideRight
                                color: "#252a31"; font.pixelSize: 11; font.weight: Font.Medium
                                verticalAlignment: Text.AlignVCenter
                            }
                            Item {
                                visible: cell.layoutMode === GuideTimeline.Full
                                anchors.fill: parent
                                anchors.leftMargin: root.cellPadding; anchors.rightMargin: root.cellPadding
                                anchors.topMargin: root.cellTopPadding; anchors.bottomMargin: root.cellTopPadding
                                Label {
                                    id: startLabel
                                    width: parent.width; height: root.timeLabelHeight
                                    text: cell.startLabel
                                    color: "#4e5651"; font.pixelSize: 10
                                }
                                Label {
                                    id: titleLabel
                                    y: startLabel.height + root.textSpacing
                                    width: parent.width; text: cell.title
                                    color: "#252a31"; font.pixelSize: 13; font.weight: Font.DemiBold
                                    textFormat: Text.PlainText; wrapMode: Text.Wrap
                                    lineHeightMode: Text.FixedHeight; lineHeight: root.titleLineHeight
                                    maximumLineCount: Math.max(1, Math.min(5, Math.floor((parent.height - y) / root.titleLineHeight)))
                                    elide: Text.ElideRight
                                }
                                Label {
                                    readonly property real descriptionLineHeight: 15
                                    readonly property real descriptionSpacing: 12
                                    y: titleLabel.y + titleLabel.height + descriptionSpacing
                                    width: parent.width; height: Math.max(0, parent.height - y)
                                    visible: height >= descriptionLineHeight * 2
                                    text: cell.modelData.description || ""
                                    color: "#4e5651"; font.pixelSize: 11
                                    textFormat: Text.PlainText; wrapMode: Text.Wrap; elide: Text.ElideRight
                                    lineHeightMode: Text.FixedHeight; lineHeight: descriptionLineHeight
                                    maximumLineCount: Math.max(1, Math.floor(height / descriptionLineHeight))
                                }
                            }
                            MouseArea { anchors.fill: parent; onClicked: {
                                root.cursorColumn = column.index
                                root.cursorKey = cell.modelData.watchKey || ""
                                root.cursorTime = Math.max(root.dayStart, cell.modelData.startAt)
                                root.forceActiveFocus()
                                root.selected(cell.modelData, cell.mapToItem(root, 0, 0), column.modelData.label)
                            } }
                        }
                    }
                    Rectangle {
                        objectName: "guideChannelHeader"
                        z: 30; y: timelineView.contentY
                        width: column.width; height: root.channelHeaderHeight; color: root.headerColor
                        Rectangle { width: root.dividerWidth; height: parent.height; color: root.headerDividerColor }
                        Row {
                            anchors.left: parent.left; anchors.leftMargin: root.channelHeaderPadding
                            anchors.verticalCenter: parent.verticalCenter; spacing: root.channelHeaderSpacing
                            Rectangle {
                                width: root.channelLogoWidth; height: 32; radius: 2
                                color: column.modelData.logo ? "#ffffff" : "transparent"
                                ChannelLogo {
                                    anchors.centerIn: parent
                                    width: 48; height: 27
                                    logoUrl: column.modelData.logo || ""
                                }
                            }
                            Label {
                                width: column.width - root.channelLogoWidth - root.channelHeaderSpacing - root.channelHeaderPadding * 2
                                anchors.verticalCenter: parent.verticalCenter
                                text: column.modelData.label.replace(/^\d+\s+/, "")
                                textFormat: Text.PlainText; color: "#e6e8e6"
                                font.pixelSize: 11; font.bold: true
                                wrapMode: Text.Wrap; maximumLineCount: 2; elide: Text.ElideRight
                            }
                        }
                    }
                }
            }
        }
        Rectangle {
            objectName: "guideCurrentTimeLine"
            visible: root.today && y >= timelineView.contentY + root.channelHeaderHeight
            z: 12; y: root.currentTimeY
            width: timelineView.contentWidth; height: 2; color: root.currentTimeColor
        }
        ScrollBar.vertical: ScrollBar {}
        ScrollBar.horizontal: ScrollBar {}
    }
    ChannelWheelArea {
        id: wheelInput
        objectName: "guideWheelArea"
        x: timelineView.x; y: timelineView.y; width: timelineView.width; height: timelineView.height
        view: timelineView
        axes: ChannelWheelArea.BothAxes
        pixelInertia: true
        onScrollStarted: horizontalScroll.stop()
        function handleWheel(event): bool {
            // Keep Shift+wheel's channel steps; touchpad pixels stay continuous.
            if (!event.pixelDelta.x && !event.pixelDelta.y && (event.modifiers & Qt.ShiftModifier)) {
                root.scrollHorizontally(event)
                return true
            }
            return scroll(event.pixelDelta, event.angleDelta)
        }
    }
}
