pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls

ComboBox {
    id: control
    hoverEnabled: true
    property url dropdownIcon: "qrc:/qt/qml/MinimalViewer/assets/icons/chevron-down.svg"
    implicitHeight: Theme.controlHeight
    opacity: enabled ? 1 : Theme.disabledOpacity
    leftPadding: Theme.spaceLg
    rightPadding: 40
    palette.button: Theme.surfaceRaised
    palette.buttonText: Theme.textPrimary
    palette.base: Theme.surface
    palette.text: Theme.textPrimary
    palette.highlight: Theme.accent
    palette.highlightedText: Theme.textOnAccent
    contentItem: Label {
        text: control.displayText
        color: Theme.textPrimary
        font.pixelSize: Theme.fontControl
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }
    indicator: Image {
        x: control.width - width - 18
        y: (control.height - height) / 2
        width: 18; height: 18
        source: control.dropdownIcon
        rotation: control.popup.visible ? 180 : 0
        Behavior on rotation { NumberAnimation { duration: Theme.moveDuration; easing.type: Easing.OutCubic } }
    }
    background: ControlSurface {
        focused: control.activeFocus
        hovered: control.hovered
        pressed: control.down
    }
    delegate: ItemDelegate {
        id: option
        required property int index
        required property var model
        width: control.width - control.popup.leftPadding - control.popup.rightPadding
        height: Theme.controlHeight
        highlighted: control.highlightedIndex === index
        contentItem: Label {
            text: option.model[control.textRole]
            color: Theme.textPrimary
            font.pixelSize: Theme.fontControl
            font.bold: control.currentIndex === option.index
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        background: Rectangle {
            radius: Theme.controlRadius
            color: option.highlighted || control.currentIndex === option.index ? Theme.selection : "transparent"
        }
    }
    popup: Popup {
        margins: 12
        y: control.height + 6
        width: control.width
        padding: Theme.spaceSm
        enter: Transition {
            ParallelAnimation {
                NumberAnimation { property: "opacity"; from: 0; to: 1; duration: Theme.colorDuration }
                NumberAnimation { property: "scale"; from: 0.97; to: 1; duration: Theme.moveDuration; easing.type: Easing.OutCubic }
            }
        }
        exit: Transition { NumberAnimation { property: "opacity"; to: 0; duration: Theme.fadeOutDuration } }
        implicitHeight: contentItem.implicitHeight + topPadding + bottomPadding
        background: Rectangle { radius: Theme.controlRadius; color: Theme.surfaceRaised; border.color: Theme.border }
        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: control.popup.visible ? control.delegateModel : null
            currentIndex: control.highlightedIndex
        }
    }
}
