pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls

SpinBox {
    id: control
    implicitWidth: 180; implicitHeight: Theme.controlHeight
    editable: true
    leftPadding: 40; rightPadding: 40
    font.pixelSize: Theme.fontControl
    opacity: enabled ? 1 : Theme.disabledOpacity
    function restoreInput(): void {
        // displayText can still contain a rejected draft when value is unchanged.
        input.text = Qt.binding(function() {
            return Number(control.value).toLocaleString(control.locale, "f", 0);
        });
    }
    function commitInput(): bool {
        if (!input.acceptableInput) return false;
        // This component edits locale-formatted integers, matching SpinBox's
        // default conversion and validator even before focus has left the field.
        value = Number.fromLocaleString(locale, input.text);
        return true;
    }
    contentItem: TextInput {
        id: input
        text: control.displayText
        font: control.font
        color: Theme.textPrimary; selectionColor: Theme.accent; selectedTextColor: Theme.textOnAccent
        horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
        readOnly: !control.editable
        validator: control.validator
        inputMethodHints: Qt.ImhDigitsOnly
        selectByMouse: true
    }
    background: ControlSurface {
        focused: control.activeFocus
        hovered: control.hovered
    }
    component Step: Rectangle {
        required property string label
        required property bool down
        required property bool hovered
        width: 36; height: control.height - 8; y: 4
        radius: Theme.controlRadius
        color: down ? Theme.selection : hovered ? Theme.selection : "transparent"
        Label { anchors.centerIn: parent; text: parent.label; font.pixelSize: Theme.fontTitle; color: Theme.textSecondary }
    }
    up.indicator: Step {
        x: control.width - width - 4
        label: "+"; down: control.up.pressed; hovered: control.up.hovered
    }
    down.indicator: Step {
        x: 4
        label: "−"; down: control.down.pressed; hovered: control.down.hovered
    }
}
