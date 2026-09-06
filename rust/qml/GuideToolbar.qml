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
    required property string band
    property Window targetWindow: null
    property url iconDirectory: "qrc:/qt/qml/MinimalViewer/assets/icons/"
    signal closeRequested
    signal settingsRequested
    signal bandRequested(string band)
    signal dayRequested(int index)
    implicitHeight: 84
    color: "#151715"
    Loader {
        anchors.fill: parent
        active: root.targetWindow !== null
        sourceComponent: WindowDragArea { targetWindow: root.targetWindow }
    }
    Loader {
        id: windowButtons
        anchors.right: parent.right; anchors.rightMargin: 18
        anchors.top: parent.top; anchors.topMargin: 18
        width: active ? 126 : 0; height: 42
        active: root.targetWindow !== null
        sourceComponent: WindowButtons { targetWindow: root.targetWindow; iconDirectory: root.iconDirectory }
    }
    RowLayout {
        anchors.left: parent.left; anchors.leftMargin: 18
        anchors.right: windowButtons.left; anchors.rightMargin: 14
        anchors.top: parent.top; anchors.topMargin: 18
        height: 42
        spacing: root.width < 980 ? 8 : 14
        IconAction {
            objectName: "closeGuide"
            iconSource: root.iconDirectory + "chevron-left.svg"
            tip: "番組表を閉じる"
            onClicked: root.closeRequested()
        }
        Label {
            visible: root.width >= 900
            text: "番組表"; color: "#e6e8e6"; font.pixelSize: 26; font.bold: true
            elide: Text.ElideRight
        }
        BroadcastTabs { rows: root.rows; value: root.band; onSelected: function(band) { root.bandRequested(band) } }
        GuideDateSelector {
            objectName: "guideDay"
            Layout.fillWidth: !compact
            Layout.minimumWidth: compact ? 202 : 82
            Layout.preferredWidth: compact ? 202 : 572
            Layout.maximumWidth: compact ? 202 : 572
            days: root.days; currentIndex: root.dayOffset
            compact: root.width < 1280
            iconDirectory: root.iconDirectory
            onSelected: function(index) { root.dayRequested(index) }
        }
        Item { Layout.fillWidth: true }
        IconAction {
            objectName: "guideSettings"
            iconSource: root.iconDirectory + "settings-2.svg"
            tip: "設定"
            onClicked: root.settingsRequested()
        }
    }
}
