pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    required property var options
    required property string value
    property string objectNamePrefix: "option-"
    property bool directionalNavigation: true
    signal selected(string value)
    signal downRequested
    readonly property int selectedIndex: options.findIndex(option => option.value === value)
    readonly property real segmentWidth: (width - 6) / Math.max(1, options.length)
    implicitWidth: options.length * 130 + 6
    implicitHeight: 40
    radius: height / 2
    color: "#b8171918"
    border.color: "#32ffffff"
    opacity: enabled ? 1 : 0.42
    function focusCurrent() {
        const tab = tabs.itemAt(Math.max(0, selectedIndex));
        if (tab) tab.forceActiveFocus(Qt.TabFocusReason);
    }
    function step(offset) {
        const index = Math.max(0, Math.min(options.length - 1, selectedIndex + offset));
        if (options[index]) { selected(options[index].value); focusCurrent(); }
    }
    Rectangle {
        x: 3 + Math.max(0, root.selectedIndex) * root.segmentWidth
        y: 3; width: root.segmentWidth; height: root.height - 6
        radius: height / 2
        visible: root.selectedIndex >= 0
        color: "#429caf9f"; border.color: "#9caf9f"
        Behavior on x { NumberAnimation { duration: 170; easing.type: Easing.OutCubic } }
    }
    Row {
        x: 3; width: parent.width - 6; height: parent.height
        Repeater {
            id: tabs
            model: root.options
            delegate: AbstractButton {
                id: tab
                required property var modelData
                objectName: root.objectNamePrefix + modelData.value
                width: root.segmentWidth; height: root.height
                Accessible.name: modelData.label
                Accessible.role: Accessible.PageTab
                checkable: true
                autoExclusive: true
                checked: root.value === modelData.value
                onClicked: root.selected(modelData.value)
                Keys.onLeftPressed: function(event) { event.accepted = root.directionalNavigation; if (event.accepted) root.step(-1); }
                Keys.onRightPressed: function(event) { event.accepted = root.directionalNavigation; if (event.accepted) root.step(1); }
                Keys.onDownPressed: function(event) { event.accepted = root.directionalNavigation; if (event.accepted) root.downRequested(); }
                Keys.onUpPressed: function(event) { event.accepted = root.directionalNavigation; }
                contentItem: Label {
                    text: tab.modelData.label
                    horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
                    color: tab.checked ? "#f4f5f3" : "#d5d8d5"
                    font.bold: tab.checked
                    elide: Text.ElideRight
                }
                background: Rectangle {
                    color: "transparent"; radius: height / 2
                    border.width: tab.visualFocus ? 1 : 0; border.color: "#9caf9f"
                }
            }
        }
    }
}
