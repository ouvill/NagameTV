pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    required property var days
    required property int currentIndex
    property bool compact: false
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    readonly property var dateLocale: Qt.locale("ja_JP")
    signal selected(int index)
    implicitWidth: compact ? 202 : 572
    implicitHeight: 40
    radius: 20; color: "#b8171918"; border.color: "#32ffffff"; clip: true
    activeFocusOnTab: true
    function selectDay(index) {
        if (index >= 0 && index < days.length && index !== currentIndex)
            selected(index)
    }
    function itemX(index) { return 3 + (index === 0 ? 0 : 62 + (index - 1) * 84) }
    function itemWidth(index) { return index === 0 ? 62 : 84 }
    function label(index) {
        return index === 0 ? "今日" : days[index] ? dateLocale.toString(new Date(days[index].start), "M/d（ddd）") : ""
    }
    function revealSelected() {
        if (!flick || compact) return
        const center = itemX(currentIndex) + itemWidth(currentIndex) / 2
        scroll.to = Math.max(0, Math.min(Math.max(0, flick.contentWidth - flick.width), center - flick.width / 2))
        scroll.restart()
    }
    onCurrentIndexChanged: {
        Qt.callLater(revealSelected)
        if (fade && compact) fade.restart()
    }
    onWidthChanged: Qt.callLater(revealSelected)
    onCompactChanged: Qt.callLater(revealSelected)
    Component.onCompleted: revealSelected()
    Keys.onLeftPressed: selectDay(currentIndex - 1)
    Keys.onRightPressed: selectDay(currentIndex + 1)
    Keys.onDownPressed: selectDay(currentIndex + 1)
    Keys.onUpPressed: selectDay(currentIndex - 1)
    Flickable {
        id: flick
        objectName: "guideDateFlick"
        visible: !root.compact
        anchors.fill: parent
        contentWidth: 6 + 62 + Math.max(0, root.days.length - 1) * 84
        contentHeight: height
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.HorizontalFlick
        NumberAnimation { id: scroll; target: flick; property: "contentX"; duration: 170; easing.type: Easing.OutCubic }
        Rectangle {
            x: root.itemX(root.currentIndex); y: 3
            width: root.itemWidth(root.currentIndex); height: flick.height - 6
            radius: height / 2; color: "#429caf9f"; border.color: "#9caf9f"
            Behavior on x { NumberAnimation { duration: 170; easing.type: Easing.OutCubic } }
            Behavior on width { NumberAnimation { duration: 170; easing.type: Easing.OutCubic } }
        }
        Row {
            x: 3; height: flick.height
            Repeater {
                model: root.days.length
                Item {
                    id: day
                    required property int index
                    objectName: "guideDate" + index
                    width: root.itemWidth(index); height: flick.height
                    Label { anchors.centerIn: parent; text: root.label(day.index); color: root.currentIndex === day.index ? "#e6e8e6" : "#d5d8d5"; font.pixelSize: 12; font.bold: root.currentIndex === day.index }
                    MouseArea { anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.selectDay(day.index) }
                }
            }
        }
    }
    Item {
        anchors.fill: parent; visible: root.compact
        Label { id: compactLabel; anchors.centerIn: parent; text: root.label(root.currentIndex); color: "#e6e8e6"; font.pixelSize: 12; font.bold: true }
        SequentialAnimation {
            id: fade
            NumberAnimation { target: compactLabel; property: "opacity"; to: .35; duration: 70; easing.type: Easing.InCubic }
            NumberAnimation { target: compactLabel; property: "opacity"; to: 1; duration: 120; easing.type: Easing.OutCubic }
        }
        Repeater {
            model: 2
            ToolButton {
                id: arrow
                required property int index
                objectName: index === 0 ? "previousGuideDay" : "nextGuideDay"
                x: index === 0 ? 0 : root.width - width
                width: 44; height: root.height
                enabled: index === 0 ? root.currentIndex > 0 : root.currentIndex < root.days.length - 1
                opacity: enabled ? 1 : .35
                Accessible.name: index === 0 ? "前日" : "翌日"
                background: Rectangle { radius: height / 2; color: arrow.hovered && arrow.enabled ? "#28ffffff" : "transparent" }
                contentItem: Item { Image { anchors.centerIn: parent; width: 16; height: 16; source: root.iconDirectory + "chevron-left.svg"; mirror: arrow.index === 1 } }
                onClicked: root.selectDay(root.currentIndex + (index === 0 ? -1 : 1))
            }
        }
    }
}
