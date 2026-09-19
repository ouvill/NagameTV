pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root
    required property string programsJson
    required property string status
    property string uiLanguage: Qt.uiLanguage
    required property string channel
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    property var rows: []
    // null means the first projection has not arrived; [] is an empty catalog.
    property string visibilityJson: "null"
    readonly property var visibleIndices: JSON.parse(visibilityJson)
    readonly property var visibleSet: visibleIndices === null ? null : new Set(visibleIndices)
    readonly property var visibleRows: rows.filter(row => row.band === band && (visibleSet === null || visibleSet.has(row.index)))
    onVisibilityJsonChanged: selectedProgram = null
    property string band: rows.length ? rows[0].band : "GR"
    required property Window targetWindow
    signal modeRequested(int mode)
    signal watchRequested(string key)
    property string watchError: ""
    signal closeRequested()
    signal dayRequested(double start, double end)
    property int dayOffset: 0
    property double baseDay: midnight()
    property point selectedPosition: Qt.point(timeline.timeRulerWidth, timeline.channelHeaderHeight)
    property string selectedChannel: ""
    property var selectedProgram: null
    onSelectedProgramChanged: watchError = ""
    readonly property var days: calendarDays(baseDay)
    readonly property var selectedWindow: days[dayOffset]
    color: "#0b0c0b"
    function midnight() {
        const date = new Date()
        date.setHours(0, 0, 0, 0)
        return date.getTime()
    }
    function calendarDays(start) {
        const result = []
        for (let i = 0; i < 7; ++i) {
            const date = new Date(start)
            date.setDate(date.getDate() + i)
            const end = new Date(date.getTime())
            end.setDate(end.getDate() + 1)
            // Keep retrieval windows independent of display language.
            result.push({ start: date.getTime(), end: end.getTime() })
        }
        return result
    }
    function requestDay() {
        if (!selectedWindow) return
        selectedProgram = null
        dayRequested(selectedWindow.start, selectedWindow.end)
    }
    function refreshSelection(columns) {
        if (!selectedProgram) return
        const key = selectedProgram.watchKey
        // Reuse the timeline's parsed snapshot. Do not retain a stale object or
        // use its old row index: updates can reorder or remove programs.
        if (typeof key === "string") {
            for (const column of columns) {
                const current = column.programs.find(program => program.watchKey === key)
                if (current) {
                    selectedProgram = current
                    return
                }
            }
        }
        selectedProgram = null
    }
    onSelectedWindowChanged: requestDay()
    Component.onCompleted: { requestDay(); timeline.forceActiveFocus() }
    onChannelChanged: selectedProgram = null
    Timer { interval: 60000; repeat: true; running: root.visible; onTriggered: root.baseDay = root.midnight() }
    ColumnLayout {
        anchors.fill: parent; spacing: 0
        GuideToolbar {
            Layout.fillWidth: true
            uiLanguage: root.uiLanguage
            rows: root.rows; days: root.days; dayOffset: root.dayOffset; band: root.band
            targetWindow: root.targetWindow; iconDirectory: root.iconDirectory
            onCloseRequested: root.closeRequested()
            onModeRequested: function(mode) { root.modeRequested(mode) }
            onDayRequested: function(index) { root.dayOffset = index }
            onBandRequested: function(band) { root.band = band; root.selectedProgram = null }
        }
        GuideTimeline {
            id: timeline
            Layout.fillWidth: true; Layout.fillHeight: true
            rows: root.visibleRows
            programsJson: root.programsJson
            dayStart: root.selectedWindow.start
            dayEnd: root.selectedWindow.end
            selectedProgram: root.selectedProgram
            onColumnsChanged: root.refreshSelection(columns)
            onSelected: function(program, cellPosition, channelLabel) {
                root.selectedPosition = cellPosition
                root.selectedChannel = channelLabel
                root.selectedProgram = program
            }
        }
    }
    Label {
        objectName: "guideStatus"
        parent: timeline
        anchors.centerIn: parent
        width: Math.max(0, timeline.width - 48)
        visible: root.programsJson === "[]"
        text: root.status
        textFormat: Text.PlainText
        horizontalAlignment: Text.AlignHCenter
        color: "#b6bab6"; font.pixelSize: 12
        wrapMode: Text.Wrap
    }
    // Block the grid behind the card, keeping main's toolbar usable.
    MouseArea {
        x: timeline.x; y: timeline.y; width: timeline.width; height: timeline.height
        visible: detailsLoader.active
        scrollGestureEnabled: false
    }
    Loader {
        id: detailsLoader
        objectName: "scheduledDetailsLoader"
        active: root.selectedProgram !== null
        sourceComponent: GuideProgramDetails {
            parent: timeline
            iconDirectory: root.iconDirectory
            uiLanguage: root.uiLanguage
            cellPosition: root.selectedPosition
            channelWidth: timeline.channelWidth
            channelLabel: root.selectedChannel
            program: root.selectedProgram
            watchError: root.watchError
            onWatchRequested: function(key) { root.watchRequested(key) }
            onClosed: root.selectedProgram = null
        }
    }
}
