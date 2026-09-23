pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls

SegmentedFrame {
    id: root
    required property var days
    required property int currentIndex
    property bool compact: false
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    property string uiLanguage: Qt.uiLanguage
    readonly property var dateLocale: Qt.locale(uiLanguage)
    signal selected(int index)
    implicitWidth: compact ? 202 : 572
    implicitHeight: 40
    clip: true
    activeFocusOnTab: true
    function selectDay(index) {
        if (index >= 0 && index < days.length && index !== currentIndex)
            selected(index)
    }
    function itemX(index) { return 3 + (index === 0 ? 0 : 62 + (index - 1) * 84) }
    function itemWidth(index) { return index === 0 ? 62 : 84 }
    function label(index) {
        return index === 0 ? qsTranslate("Main", "Today") : days[index] ? new Date(days[index].start).toLocaleDateString(dateLocale, qsTranslate("Main", "ddd, MMM d")) : ""
    }
    function revealSelected() {
        if (!flick || compact) return
        const center = itemX(currentIndex) + itemWidth(currentIndex) / 2
        scroll.to = Math.max(0, Math.min(Math.max(0, flick.contentWidth - flick.width), center - flick.width / 2))
        scroll.restart()
    }
    // Deferred layout work belongs to this selector and is cancelled with it.
    Timer { id: revealTimer; interval: 0; onTriggered: root.revealSelected() }
    onCurrentIndexChanged: {
        revealTimer.restart()
        if (fade && compact) fade.restart()
    }
    onWidthChanged: revealTimer.restart()
    onCompactChanged: revealTimer.restart()
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
        NumberAnimation { id: scroll; target: flick; property: "contentX"; duration: Theme.moveDuration; easing.type: Easing.OutCubic }
        Rectangle {
            x: root.itemX(root.currentIndex); y: 3
            width: root.itemWidth(root.currentIndex); height: flick.height - 6
            radius: root.segmentCornerRadius; color: Theme.selection; border.color: Theme.accent
            Behavior on x { NumberAnimation { duration: Theme.moveDuration; easing.type: Easing.OutCubic } }
            Behavior on width { NumberAnimation { duration: Theme.moveDuration; easing.type: Easing.OutCubic } }
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
                    Label { anchors.centerIn: parent; text: root.label(day.index); color: root.currentIndex === day.index ? Theme.textPrimary : Theme.textSecondary; font.pixelSize: Theme.fontCaption; font.bold: root.currentIndex === day.index }
                    MouseArea { anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: root.selectDay(day.index) }
                }
            }
        }
    }
    Item {
        anchors.fill: parent; visible: root.compact
        Label { id: compactLabel; anchors.centerIn: parent; text: root.label(root.currentIndex); color: Theme.textPrimary; font.pixelSize: Theme.fontCaption; font.bold: true }
        SequentialAnimation {
            id: fade
            NumberAnimation { target: compactLabel; property: "opacity"; to: .35; duration: Theme.pressDuration; easing.type: Easing.InCubic }
            NumberAnimation { target: compactLabel; property: "opacity"; to: 1; duration: Theme.fadeInDuration; easing.type: Easing.OutCubic }
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
                opacity: enabled ? 1 : Theme.disabledOpacity
                Accessible.name: index === 0 ? qsTranslate("Viewer", "Previous day") : qsTranslate("Viewer", "Next day")
                background: Rectangle { radius: root.outerCornerRadius; color: arrow.hovered && arrow.enabled ? Theme.overlayHover : "transparent" }
                contentItem: Item { Image { anchors.centerIn: parent; width: 16; height: 16; source: root.iconDirectory + "chevron-left.svg"; mirror: arrow.index === 1 } }
                onClicked: root.selectDay(root.currentIndex + (index === 0 ? -1 : 1))
            }
        }
    }
}
