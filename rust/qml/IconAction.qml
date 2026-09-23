import QtQuick
import MinimalViewer
import QtQuick.Controls

ToolButton {
    id: control
    required property url iconSource
    required property string tip
    property string iconLabel: ""
    property bool active: false
    enum Emphasis { Secondary, Destructive }
    property int emphasis: IconAction.Secondary
    property int iconSize: Theme.iconSize
    property bool toolTipEnabled: true
    // Player overlays use bare icons until hover, focus, or selection.
    flat: false
    // Animate the visuals; keep the hit area still while the pointer is down.
    property real feedbackScale: down ? Theme.pressScale : 1
    Behavior on feedbackScale {
        NumberAnimation { duration: control.down ? Theme.pressDuration : Theme.moveDuration; easing.type: Easing.OutCubic }
    }
    hoverEnabled: true
    opacity: enabled ? 1 : Theme.disabledOpacity
    implicitWidth: Theme.iconButtonSize
    implicitHeight: Theme.iconButtonSize
    Accessible.name: tip
    background: Rectangle {
        scale: control.feedbackScale
        radius: height / 2
        color: control.emphasis === IconAction.Destructive && (control.down || control.hovered) ? Theme.destructive
            : control.down ? Theme.overlayPressed : control.hovered ? Theme.overlayHover
            : control.active ? Theme.selection : control.flat ? "transparent" : Theme.overlaySurface
        border.color: control.visualFocus || control.active ? Theme.accent : control.flat ? "transparent" : Theme.overlayBorder
        Behavior on color { ColorAnimation { duration: Theme.colorDuration } }
        Behavior on border.color { ColorAnimation { duration: Theme.colorDuration } }
    }
    contentItem: Item {
        scale: control.feedbackScale
        Image {
            anchors.centerIn: parent
            width: control.iconSize
            height: control.iconSize
            sourceSize: Qt.size(control.iconSize, control.iconSize)
            source: control.iconSource
        }
        Label {
            anchors.centerIn: parent
            anchors.verticalCenterOffset: -1
            text: control.iconLabel
            visible: text.length > 0
            color: Theme.textPrimary
            font.pixelSize: Theme.fontMicro
            font.bold: true
        }
    }
    ThemedToolTip {
        objectName: "actionToolTip"
        parent: control
        visible: control.toolTipEnabled && control.hovered
        text: control.tip
        delay: 0
        timeout: 3000
        x: (control.width - implicitWidth) / 2
        y: -implicitHeight - 10
        padding: Theme.spaceSm
    }
}
