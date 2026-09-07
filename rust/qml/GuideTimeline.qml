pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Item {
    id: root
    required property var rows
    required property string programsJson
    required property double dayStart
    required property double dayEnd
    property var selectedProgram: null
    signal selected(var program, point cellPosition, string channelLabel)
    readonly property real channelWidth: 222
    readonly property real pixelsPerMinute: 2.4
    readonly property var columns: JSON.parse(programsJson)
    property double now: Date.now()
    readonly property bool today: now >= dayStart && now < dayEnd
    readonly property real currentTimeY: 88 + (now - dayStart) / 60000 * pixelsPerMinute
    // Boundary bindings can update separately during a date change.
    readonly property double dayDuration: Math.max(0, dayEnd - dayStart)
    readonly property int hours: Math.ceil(dayDuration / 3600000)
    // Keep only identity and position; delegates can be unloaded offscreen and
    // the EPG snapshot can be replaced while keyboard navigation is active.
    property int cursorColumn: 0
    property string cursorKey: ""
    property double cursorTime: dayStart
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
        horizontalScroll.stop()
        view.cancelFlick()
        const left = cursorColumn * channelWidth
        view.contentX = Math.max(0, Math.min(Math.max(0, view.contentWidth - view.width),
            left < view.contentX ? left : Math.max(view.contentX, left + channelWidth - view.width)))
        if (!cursorProgram) return
        const top = 88 + (Math.max(dayStart, cursorProgram.startAt) - dayStart) / 60000 * pixelsPerMinute
        // Align the start of long programs below the sticky channel header.
        if (top < view.contentY + 88 || top + 24 > view.contentY + view.height)
            view.contentY = Math.max(0, Math.min(Math.max(0, view.contentHeight - view.height), top - 88))
    }
    function navigate(key) {
        if (!rows.length) return
        if (!cursorProgram) chooseProgram(today ? now : dayStart)
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
            const top = 88 + (Math.max(dayStart, cursorProgram.startAt) - dayStart) / 60000 * pixelsPerMinute
            selected(cursorProgram, Qt.point(view.x + cursorColumn * channelWidth - view.contentX, top - view.contentY), rows[cursorColumn].label)
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
        view.cancelFlick()
        horizontalScroll.stop()
        horizontalScroll.from = view.contentX
        horizontalScroll.to = Math.max(0, Math.min(Math.max(0, view.contentWidth - view.width),
            view.contentX + (delta < 0 ? channelWidth : -channelWidth)))
        horizontalScroll.start()
        event.accepted = true
    }
    NumberAnimation { id: horizontalScroll; target: view; property: "contentX"; duration: 150; easing.type: Easing.OutCubic }
    function resetPosition() {
        view.contentY = now >= dayStart && now < dayEnd
            ? Math.max(0, Math.min(view.contentHeight - view.height, 88 + (now - dayStart) / 60000 * pixelsPerMinute - view.height * 0.34)) : 0
    }
    onDayStartChanged: { cursorKey = ""; Qt.callLater(resetPosition) }
    onRowsChanged: {
        cursorColumn = 0
        cursorKey = ""
        horizontalScroll.stop()
        view.cancelFlick()
        view.contentX = 0
    }
    Component.onCompleted: resetPosition()
    Timer { interval: 1000; repeat: true; running: root.visible; onTriggered: root.now = Date.now() }
    Item {
        width: 104; height: parent.height; clip: true
        Repeater {
            model: root.hours + 1
            Label {
                required property int index
                x: 24; y: 80 + index * 60 * root.pixelsPerMinute - 8 - view.contentY
                text: Qt.formatTime(new Date(root.dayStart + index * 3600000), "hh:mm")
                color: "#b6bab6"; font.pixelSize: 12; font.bold: true
            }
        }
        Rectangle {
            objectName: "guideCurrentTimeBadge"
            visible: root.today
            x: 20; y: root.currentTimeY - height / 2 - view.contentY
            width: 68; height: 24; radius: 12; color: "#9caf9f"
            Label {
                anchors.centerIn: parent
                text: Qt.formatTime(new Date(root.now), "hh:mm")
                color: "#17201a"; font.pixelSize: 11; font.bold: true
            }
        }
    }
    Flickable {
        id: view
        objectName: "guideTimeline"
        x: 104; width: Math.max(0, parent.width - 104); height: parent.height; clip: true
        contentWidth: Math.max(width, root.rows.length * root.channelWidth)
        contentHeight: 88 + root.dayDuration / 60000 * root.pixelsPerMinute
        boundsBehavior: Flickable.StopAtBounds
        Repeater {
            model: root.hours + 1
            Rectangle {
                required property int index
                y: 88 + index * 60 * root.pixelsPerMinute
                width: view.contentWidth; height: 1; color: "#20ffffff"
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
                width: root.channelWidth - 4; height: view.contentHeight
                active: x + width >= view.contentX - root.channelWidth
                    && x <= view.contentX + view.width + root.channelWidth
                sourceComponent: Item {
                    width: column.width; height: column.height
                    Repeater {
                        model: root.schedule(column.modelData.index)
                        Rectangle {
                            id: cell
                            required property var modelData
                            objectName: "guideCell"
                            readonly property double begin: Math.max(root.dayStart, modelData.startAt)
                            readonly property double end: Math.min(root.dayEnd, modelData.startAt + modelData.duration)
                            y: 88 + (begin - root.dayStart) / 60000 * root.pixelsPerMinute
                            width: column.width
                            height: Math.max(24, (end - begin) / 60000 * root.pixelsPerMinute - 4)
                            color: root.genreColor(modelData.genre)
                            readonly property bool highlighted: modelData === root.selectedProgram
                                || (root.activeFocus && column.index === root.cursorColumn && root.cursorKey.length > 0 && modelData.watchKey === root.cursorKey)
                            border.width: highlighted ? 4 : 1
                            border.color: highlighted ? "#9caf9f" : "#5b625e"
                            Column {
                                anchors.fill: parent; anchors.margins: 10; spacing: 5
                                Label {
                                    width: parent.width; text: cell.modelData.name || qsTranslate("Viewer", "No program information")
                                    color: "#1b201d"; font.pixelSize: 13; font.bold: true
                                    textFormat: Text.PlainText; wrapMode: Text.Wrap
                                    maximumLineCount: Math.max(1, Math.floor((parent.height - 22) / 17)); elide: Text.ElideRight
                                }
                                Label {
                                    visible: parent.height > 46
                                    text: Qt.formatTime(new Date(cell.modelData.startAt), "hh:mm") + "–" + Qt.formatTime(new Date(cell.modelData.startAt + cell.modelData.duration), "hh:mm")
                                    color: "#4e5651"; font.pixelSize: 10
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
                        z: 30; y: view.contentY; width: column.width; height: 88; color: "#151715"
                        Row {
                            anchors.left: parent.left; anchors.leftMargin: 14; anchors.verticalCenter: parent.verticalCenter; spacing: 8
                            ChannelLogo { width: 56; height: 32; logoUrl: column.modelData.logo || "" }
                            Label { width: root.channelWidth - 94; anchors.verticalCenter: parent.verticalCenter; text: column.modelData.label.replace(/^\d+\s+/, ""); textFormat: Text.PlainText; color: "#e6e8e6"; font.bold: true; elide: Text.ElideRight }
                        }
                    }
                }
            }
        }
        Rectangle {
            objectName: "guideCurrentTimeLine"
            visible: root.today && y >= view.contentY + 88
            z: 12; y: root.currentTimeY
            width: view.contentWidth; height: 2; color: "#9caf9f"
        }
        ScrollBar.vertical: ScrollBar {}
        ScrollBar.horizontal: ScrollBar {}
    }
    MouseArea {
        objectName: "guideWheelArea"
        x: view.x; y: view.y; width: view.width; height: view.height
        acceptedButtons: Qt.LeftButton
        propagateComposedEvents: true
        scrollGestureEnabled: false
        onPressed: function(mouse) { mouse.accepted = false }
        onClicked: function(mouse) { mouse.accepted = false }
        onWheel: function(event) {
            if (event.modifiers & Qt.ShiftModifier) root.scrollHorizontally(event)
            else event.accepted = false
        }
    }
}
