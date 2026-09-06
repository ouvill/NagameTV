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
    signal selected(var program)
    readonly property real channelWidth: 222
    readonly property real pixelsPerMinute: 2.4
    readonly property var columns: JSON.parse(programsJson)
    property double now: Date.now()
    // Boundary bindings can update separately during a date change.
    readonly property double dayDuration: Math.max(0, dayEnd - dayStart)
    readonly property int hours: Math.ceil(dayDuration / 3600000)
    function schedule(index) {
        const column = columns.find(column => column.index === index)
        return column ? column.programs : []
    }
    function resetPosition() {
        view.contentX = 0
        view.contentY = now >= dayStart && now < dayEnd
            ? Math.max(0, Math.min(view.contentHeight - view.height, 88 + (now - dayStart) / 60000 * pixelsPerMinute - 80)) : 0
    }
    onDayStartChanged: Qt.callLater(resetPosition)
    onRowsChanged: view.contentX = 0
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
                            color: "#f0f0f0"
                            border.width: modelData === root.selectedProgram ? 4 : 1
                            border.color: modelData === root.selectedProgram ? "#9caf9f" : "#5b625e"
                            Column {
                                anchors.fill: parent; anchors.margins: 10; spacing: 5
                                Label {
                                    width: parent.width; text: cell.modelData.name || "番組情報がありません"
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
                            MouseArea { anchors.fill: parent; onClicked: root.selected(cell.modelData) }
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
            visible: root.now >= root.dayStart && root.now < root.dayEnd
            z: 12; y: 88 + (root.now - root.dayStart) / 60000 * root.pixelsPerMinute
            width: view.contentWidth; height: 2; color: "#9caf9f"
        }
        ScrollBar.vertical: ScrollBar {}
        ScrollBar.horizontal: ScrollBar {}
    }
}
