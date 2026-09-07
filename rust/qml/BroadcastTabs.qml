pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    required property var rows
    required property string value
    property string uiLanguage: Qt.uiLanguage
    signal selected(string band)
    property bool directionalNavigation: false
    signal downRequested
    function focusCurrent() {
        const tab = tabs.itemAt(Math.max(0, selectedIndex));
        if (tab)
            tab.forceActiveFocus(Qt.TabFocusReason);
    }
    function step(offset) {
        const index = Math.max(0, Math.min(options.length - 1, selectedIndex + offset));
        if (options[index]) {
            selected(options[index].value);
            focusCurrent();
        }
    }
    readonly property var options: [
        {
            value: "GR",
            label: qsTranslate("Main", "Terrestrial")
        },
        {
            value: "BS",
            label: "BS"
        },
        {
            value: "CS",
            label: "CS"
        },
        {
            value: "SKY",
            label: "SKY"
        },
        {
            value: "OTHER",
            label: qsTranslate("Viewer", "Other")
        }
    ].filter(option => rows.some(row => row.band === option.value))
    readonly property int selectedIndex: options.findIndex(option => option.value === value)
    implicitWidth: Math.max(82, options.length * (uiLanguage === "en" ? 96 : 76) + 6)
    readonly property real segmentWidth: (width - 6) / Math.max(1, options.length)
    implicitHeight: 40
    radius: 20
    color: "#b8171918"
    border.color: "#32ffffff"
    visible: options.length > 0
    Rectangle {
        x: 3 + Math.max(0, root.selectedIndex) * root.segmentWidth
        y: 3
        width: root.segmentWidth
        height: root.height - 6
        radius: height / 2
        visible: root.selectedIndex >= 0
        color: "#429caf9f"
        border.color: "#9caf9f"
        Behavior on x {
            NumberAnimation {
                duration: 170
                easing.type: Easing.OutCubic
            }
        }
    }
    Row {
        x: 3
        width: parent.width - 6
        height: parent.height
        Repeater {
            id: tabs
            model: root.options
            delegate: AbstractButton {
                id: tab
                required property var modelData
                objectName: "band-" + modelData.value
                width: root.segmentWidth
                height: root.height
                Accessible.name: modelData.label
                onClicked: root.selected(modelData.value)
                Keys.onLeftPressed: function(event) {
                    event.accepted = root.directionalNavigation;
                    if (event.accepted) root.step(-1);
                }
                Keys.onRightPressed: function(event) {
                    event.accepted = root.directionalNavigation;
                    if (event.accepted) root.step(1);
                }
                Keys.onDownPressed: function(event) {
                    event.accepted = root.directionalNavigation;
                    if (event.accepted) root.downRequested();
                }
                Keys.onUpPressed: function(event) {
                    event.accepted = root.directionalNavigation;
                }
                contentItem: Item {
                    Label {
                        anchors.centerIn: parent
                        text: tab.modelData.label
                        color: root.value === tab.modelData.value ? "#f4f5f3" : "#d5d8d5"
                        font.bold: root.value === tab.modelData.value
                    }
                }
                background: Rectangle {
                    color: "transparent"
                    radius: 20
                    border.width: tab.visualFocus ? 1 : 0
                    border.color: "#9caf9f"
                }
            }
        }
    }
}
