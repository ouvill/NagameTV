pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

Rectangle {
    id: root
    required property var rows
    required property string value
    signal selected(string band)
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
    implicitWidth: Math.max(82, options.length * 76 + 6)
    implicitHeight: 40
    radius: 20
    color: "#b8171918"
    border.color: "#32ffffff"
    visible: options.length > 0
    Rectangle {
        x: 3 + Math.max(0, root.selectedIndex) * 76
        y: 3
        width: 76
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
        height: parent.height
        Repeater {
            model: root.options
            delegate: AbstractButton {
                id: tab
                required property var modelData
                objectName: "band-" + modelData.value
                width: 76
                height: root.height
                Accessible.name: modelData.label
                onClicked: root.selected(modelData.value)
                contentItem: Label {
                    text: tab.modelData.label
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    color: root.value === tab.modelData.value ? "#f4f5f3" : "#d5d8d5"
                    font.bold: root.value === tab.modelData.value
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
