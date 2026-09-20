pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: root
    objectName: "guideToolbar"
    required property var rows
    required property var days
    required property int dayOffset
    property string uiLanguage: Qt.uiLanguage
    required property string band
    required property Window targetWindow
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    signal closeRequested
    signal modeRequested(int mode)
    signal bandRequested(string band)
    signal dayRequested(int index)
    readonly property real singleRowHeight: navigation.headerHeight
    readonly property real secondRowHeight: 44
    readonly property real sideMargin: navigation.edgeMargin
    readonly property real controlSpacing: 8
    readonly property real compactDateWidth: 202
    readonly property real expandedDateWidth: 572
    readonly property real bandWidth: broadcastTabs.visible ? broadcastTabs.implicitWidth + controlSpacing : 0
    readonly property bool twoRows: width < sideMargin * 2 + heading.width + navigation.width + helpButton.width + controlSpacing
        + bandWidth + compactDateWidth + controlSpacing * 2
    implicitHeight: singleRowHeight + (twoRows ? secondRowHeight : 0)
    color: "#151715"
    Loader {
        anchors.fill: parent
        active: root.targetWindow !== null
        sourceComponent: WindowDragArea { targetWindow: root.targetWindow }
    }
    Row {
        id: heading
        x: root.sideMargin
        y: (root.singleRowHeight - height) / 2
        height: 42
        spacing: root.controlSpacing
        IconAction {
            objectName: "closeGuide"
            flat: true
            iconSource: root.iconDirectory + "chevron-left.svg"
            tip: qsTranslate("Viewer", "Close program guide")
            onClicked: root.closeRequested()
        }
        Label {
            visible: root.width >= 900
            anchors.verticalCenter: parent.verticalCenter
            text: qsTranslate("Main", "Program guide"); color: "#e6e8e6"; font.pixelSize: 18; font.bold: true
        }
    }
    RowLayout {
        id: filters
        objectName: "guideFilters"
        x: root.twoRows ? root.sideMargin : heading.x + heading.width + root.controlSpacing
        y: root.twoRows ? root.singleRowHeight : (root.singleRowHeight - height) / 2
        width: Math.max(0, (root.twoRows ? root.width - root.sideMargin : helpButton.x - root.controlSpacing) - x)
        height: 40
        spacing: root.controlSpacing
        BroadcastTabs {
            id: broadcastTabs
            Layout.preferredWidth: implicitWidth
            rows: root.rows; value: root.band; uiLanguage: root.uiLanguage
            onSelected: function(band) { root.bandRequested(band) }
        }
        GuideDateSelector {
            objectName: "guideDay"
            Layout.minimumWidth: root.compactDateWidth
            Layout.preferredWidth: compact ? root.compactDateWidth : root.expandedDateWidth
            Layout.maximumWidth: Layout.preferredWidth
            days: root.days; currentIndex: root.dayOffset
            uiLanguage: root.uiLanguage
            compact: filters.width < root.bandWidth + root.expandedDateWidth
            iconDirectory: root.iconDirectory
            onSelected: function(index) { root.dayRequested(index) }
        }
        Item { Layout.fillWidth: true }
    }
    IconAction {
        id: helpButton
        objectName: "guideHelp"
        anchors { right: navigation.left; rightMargin: root.controlSpacing; verticalCenter: navigation.verticalCenter }
        flat: true
        iconSource: root.iconDirectory + "info.svg"
        tip: qsTranslate("Main", "←→ Channels   ↑↓ Time   Enter Details")
        toolTipEnabled: false
        onClicked: help.open()
    }
    ModeNavigation {
        id: navigation
        objectName: "guideModeNavigation"
        targetWindow: root.targetWindow
        iconDirectory: root.iconDirectory
        mode: ModeNavigation.Guide
        guideEnabled: true
        onModeRequested: function(mode) { root.modeRequested(mode) }
    }
    Popup {
        id: help
        objectName: "guideHelpPopup"
        x: root.width - width - root.sideMargin
        y: root.height
        width: Math.min(420, root.width - root.sideMargin * 2)
        padding: 12
        focus: true
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        background: Rectangle { radius: 8; color: "#151715"; border.color: "#38ffffff" }
        contentItem: Label {
            text: qsTranslate("Main", "←→ Channels   ↑↓ Time   Enter Details")
            color: "#e6e8e6"; font.pixelSize: 12
            wrapMode: Text.Wrap
        }
    }
}
