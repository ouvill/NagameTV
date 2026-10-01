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
    property bool focusLabelBelow: false
    property int focusLabelAlignment: Qt.AlignHCenter
    // Player overlays use bare icons until hover, focus, or selection.
    flat: false
    // Animate the visuals; keep the hit area still while the pointer is down.
    property real feedbackScale: down ? Theme.pressScale : enabled && focusVisible ? Theme.focusScale : 1
    Behavior on feedbackScale {
        NumberAnimation { duration: control.down ? Theme.pressDuration : Theme.moveDuration; easing.type: Easing.OutCubic }
    }
    hoverEnabled: true
    readonly property bool focusVisible: keyboard.focused
    ButtonKeys { id: keyboard; button: control }
    Keys.onReturnPressed: function(event) { keyboard.confirm(event); }
    Keys.onEnterPressed: function(event) { keyboard.confirm(event); }
    opacity: enabled ? 1 : Theme.disabledOpacity
    implicitWidth: Theme.iconButtonSize
    implicitHeight: Theme.iconButtonSize
    Accessible.name: tip
    background: Rectangle {
        scale: control.feedbackScale
        radius: height / 2
        color: control.emphasis === IconAction.Destructive && (control.down || control.hovered) ? Theme.destructive
            : control.down ? Theme.overlayPressed : control.active ? Theme.selection
            : control.hovered || control.focusVisible ? Theme.overlayHover : control.flat ? "transparent" : Theme.overlaySurface
        border.color: control.flat ? "transparent" : Theme.overlayBorder
        Behavior on color { ColorAnimation { duration: Theme.colorDuration } }
        Behavior on border.color { ColorAnimation { duration: Theme.colorDuration } }
        FocusOutline { focused: control.focusVisible; cornerRadius: parent.radius }
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
        visible: control.toolTipEnabled && control.hovered && !control.focusVisible
        text: control.tip
        delay: 0
        timeout: 3000
        x: (control.width - implicitWidth) / 2
        y: -implicitHeight - 10
        padding: Theme.spaceSm
    }
    Label {
        objectName: "actionFocusLabel"
        visible: control.toolTipEnabled && control.focusVisible
        text: control.tip
        textFormat: Text.PlainText
        font.pixelSize: Theme.fontBody
        color: Theme.textPrimary
        padding: Theme.spaceSm
        x: control.focusLabelAlignment === Qt.AlignRight ? control.width - implicitWidth
            : (control.width - implicitWidth) / 2
        y: control.focusLabelBelow ? control.height + Theme.spaceSm : -implicitHeight - Theme.spaceSm
        z: 1
        background: PanelSurface { radius: Theme.controlRadius; color: Theme.overlaySurface }
    }
}
