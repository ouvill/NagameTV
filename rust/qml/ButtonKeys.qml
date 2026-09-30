pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Templates as Templates

// Qt may restore a popup's opener with OtherFocusReason. Preserve the input
// origin for its focus cue without moving focus or interrupting a held key.
Item {
    id: root
    required property Templates.AbstractButton button
    enum Origin { Pointer, Keyboard }
    readonly property bool focused: button.activeFocus && state.origin === ButtonKeys.Keyboard
    QtObject {
        id: state
        property int origin: ButtonKeys.Pointer
        property bool confirming: false
    }
    Connections {
        target: root.button
        function onFocusReasonChanged() {
            if ([Qt.TabFocusReason, Qt.BacktabFocusReason, Qt.ShortcutFocusReason].includes(root.button.focusReason))
                state.origin = ButtonKeys.Keyboard;
        }
        function onPressed() {
            if (!state.confirming && root.button.focusReason === Qt.MouseFocusReason)
                state.origin = ButtonKeys.Pointer;
        }
    }
    function confirm(event) {
        if (event.isAutoRepeat) return;
        state.origin = ButtonKeys.Keyboard;
        state.confirming = true;
        try { button.click(); }
        finally { state.confirming = false; }
    }
}
