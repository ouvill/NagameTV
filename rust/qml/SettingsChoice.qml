pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

ComboBox {
    id: control
    hoverEnabled: true
    property url dropdownIcon: "qrc:/qt/qml/MinimalViewer/assets/icons/chevron-down.svg"
    implicitHeight: 44
    leftPadding: 14
    rightPadding: 40
    palette.button: "#222622"
    palette.buttonText: "#f4f5f3"
    palette.base: "#151715"
    palette.text: "#f4f5f3"
    palette.highlight: "#9caf9f"
    palette.highlightedText: "#17201a"
    contentItem: Label {
        text: control.displayText
        color: "#f4f5f3"
        font.pixelSize: 16
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    indicator: Image {
        x: control.width - width - 18
        y: (control.height - height) / 2
        width: 18; height: 18
        source: control.dropdownIcon
        rotation: control.popup.visible ? 180 : 0
        Behavior on rotation { NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
    }
    background: Rectangle {
        radius: 8
        color: control.down ? "#344238" : control.hovered ? "#2a302b" : "#222622"
        border.color: control.activeFocus ? "#9caf9f" : "#3b423c"
        Behavior on color { ColorAnimation { duration: 100 } }
        Behavior on border.color { ColorAnimation { duration: 100 } }
    }
    delegate: ItemDelegate {
        id: option
        required property int index
        required property string modelData
        width: control.width - 12
        height: 44
        highlighted: control.highlightedIndex === index
        contentItem: Label {
            text: option.modelData
            color: "#f4f5f3"
            font.pixelSize: 16
            font.bold: control.currentIndex === option.index
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        background: Rectangle {
            radius: 8
            color: option.highlighted ? "#389caf9f"
                : control.currentIndex === option.index ? "#209caf9f" : "transparent"
        }
    }
    popup: Popup {
        margins: 12
        y: control.height + 6
        width: control.width
        padding: 6
        enter: Transition {
            ParallelAnimation {
                NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 100 }
                NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: 140; easing.type: Easing.OutCubic }
            }
        }
        exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: 80 } }
        implicitHeight: contentItem.implicitHeight + topPadding + bottomPadding
        background: Rectangle { radius: 8; color: "#222622"; border.color: "#3b423c" }
        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: control.popup.visible ? control.delegateModel : null
            currentIndex: control.highlightedIndex
        }
    }
}
