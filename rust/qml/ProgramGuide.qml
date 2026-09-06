pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root
    required property string programsJson
    required property string status
    required property string channel
    signal refreshRequested()
    signal closeRequested()
    signal dayRequested(double start, double end)
    property int dayOffset: 0
    property double baseDay: midnight()
    property var selectedProgram: null
    readonly property var days: calendarDays(baseDay)
    readonly property var selectedWindow: days[dayOffset]
    color: "#24282e"
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
        anchors.fill: parent; anchors.margins: 10
        RowLayout {
            Label { text: "番組表 — " + root.channel; color: "white"; Layout.fillWidth: true; elide: Text.ElideRight }
            Button { text: "更新"; onClicked: root.refreshRequested() }
            Button { text: "閉じる"; onClicked: root.closeRequested() }
        }
        ComboBox {
            objectName: "guideDay"
            Layout.fillWidth: true
            model: root.days
            textRole: "label"
            currentIndex: root.dayOffset
            onActivated: root.dayOffset = currentIndex
        }
        Label { text: root.status; color: "#cccccc" }
        ListView {
            id: programs
            objectName: "guidePrograms"
            Layout.fillWidth: true; Layout.fillHeight: true
            clip: true; spacing: 8
            model: JSON.parse(root.programsJson)
            onModelChanged: positionViewAtBeginning()
            delegate: ItemDelegate {
                id: card
                required property var modelData
                width: programs.width
                padding: 8
                onClicked: root.selectedProgram = modelData
                Accessible.name: modelData.name || "番組名未取得"
                background: Rectangle { color: card.down ? "#465363" : "#333940" }
                contentItem: Column {
                    spacing: 4
                    Label {
                        width: parent.width; color: "#a6caff"
                        text: Qt.formatDateTime(new Date(card.modelData.startAt), "MM/dd hh:mm")
                            + " – " + Qt.formatDateTime(new Date(card.modelData.startAt + card.modelData.duration), "hh:mm")
                    }
                    Label { width: parent.width; color: "white"; text: card.modelData.name || "番組名未取得"; textFormat: Text.PlainText; wrapMode: Text.Wrap; font.bold: true }
                    Label { width: parent.width; color: "#dddddd"; text: card.modelData.description || ""; textFormat: Text.PlainText; wrapMode: Text.Wrap; maximumLineCount: 4; elide: Text.ElideRight }
                }
            }
            Label { anchors.centerIn: parent; visible: programs.count === 0; text: "表示できる番組がありません"; color: "white" }
            ScrollBar.vertical: ScrollBar {}
        }
    }
    Loader {
        objectName: "scheduledDetailsLoader"
        active: root.selectedProgram !== null
        sourceComponent: ProgramDetails {
            program: root.selectedProgram
            onClosed: root.selectedProgram = null
        }
    }
}
