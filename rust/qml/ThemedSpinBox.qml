pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls

SpinBox {
    id: control
    implicitWidth: 180; implicitHeight: 44
    editable: true
    leftPadding: 40; rightPadding: 40
    font.pixelSize: 16
    opacity: enabled ? 1 : 0.42
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
        color: "#f4f5f3"; selectionColor: "#9caf9f"; selectedTextColor: "#151715"
        horizontalAlignment: Text.AlignHCenter; verticalAlignment: Text.AlignVCenter
        readOnly: !control.editable
        validator: control.validator
        inputMethodHints: Qt.ImhDigitsOnly
        selectByMouse: true
    }
    background: Rectangle {
        radius: 8; color: "#222622"
        border.color: control.activeFocus ? "#9caf9f" : "#3b423c"
    }
    component Step: Rectangle {
        required property string label
        required property bool down
        required property bool hovered
        width: 36; height: control.height - 8; y: 4
        radius: 8
        color: down ? "#429caf9f" : hovered ? "#229caf9f" : "transparent"
        Label { anchors.centerIn: parent; text: parent.label; font.pixelSize: 22; color: "#c7d3c9" }
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
