pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root
    required property string programsJson
    required property string status
    required property string channel
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    property var rows: []
    property string band: rows.length ? rows[0].band : "GR"
    property Window targetWindow: null
    signal settingsRequested()
    signal watchRequested(string key)
    property string watchError: ""
    signal closeRequested()
    signal dayRequested(double start, double end)
    property int dayOffset: 0
    property double baseDay: midnight()
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
            result.push({ label: Qt.formatDateTime(date, "MM/dd (ddd)"), start: date.getTime(), end: end.getTime() })
        }
        return result
    }
    function requestDay() {
        if (!selectedWindow) return
        selectedProgram = null
        dayRequested(selectedWindow.start, selectedWindow.end)
    }
    onSelectedWindowChanged: requestDay()
    Component.onCompleted: requestDay()
    onChannelChanged: selectedProgram = null
    Timer { interval: 60000; repeat: true; running: root.visible; onTriggered: root.baseDay = root.midnight() }
    ColumnLayout {
        anchors.fill: parent; spacing: 0
        GuideToolbar {
            Layout.fillWidth: true
            Layout.preferredHeight: 84
            rows: root.rows; days: root.days; dayOffset: root.dayOffset; band: root.band
            targetWindow: root.targetWindow; iconDirectory: root.iconDirectory
            onCloseRequested: root.closeRequested()
            onSettingsRequested: root.settingsRequested()
            onDayRequested: function(index) { root.dayOffset = index }
            onBandRequested: function(band) { root.band = band; root.selectedProgram = null }
        }
        GuideTimeline {
            Layout.fillWidth: true; Layout.fillHeight: true
            rows: root.rows.filter(row => row.band === root.band)
            programsJson: root.programsJson
            dayStart: root.selectedWindow.start
            dayEnd: root.selectedWindow.end
            selectedProgram: root.selectedProgram
            onSelected: function(program) { root.selectedProgram = program }
        }
    }

    Label {
        anchors.bottom: parent.bottom; anchors.left: parent.left; anchors.margins: 18
        visible: root.programsJson === "[]"
        text: root.status; color: "#b6bab6"; textFormat: Text.PlainText
    }
    Loader {
        objectName: "scheduledDetailsLoader"
        active: root.selectedProgram !== null
        sourceComponent: ProgramDetails {
            program: root.selectedProgram
            watchEnabled: true
            watchError: root.watchError
            onWatchRequested: function(key) { root.watchRequested(key) }
            onClosed: root.selectedProgram = null
        }
    }
}
