pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Templates as Templates
import MinimalViewer

// A form is traversed by rows. Its controls keep their own horizontal value
// adjustment and confirmation; vertical keys never change a slider's value.
QtObject {
    id: root
    required property ScrollView scrollView
    required property list<Item> fields
    readonly property Item focusItem: scrollView.Window.window ? scrollView.Window.window.activeFocusItem : null
    readonly property Templates.Control focusedControl: focusItem as Templates.Control
    readonly property bool keyboardFocused: focusItem instanceof ActionButton
        ? (focusItem as ActionButton).focusVisible : focusItem instanceof ToggleSwitch
        ? (focusItem as ToggleSwitch).focusVisible : focusedControl !== null && focusedControl.visualFocus
    readonly property int currentIndex: fields.findIndex(field => containsFocus(field))
    signal boundaryReached(int key)
    readonly property QtObject focusMemory: QtObject { id: memory; property Item field: null }
    function containsFocus(field: Item): bool {
        for (let item = focusItem; item; item = item.parent) {
            if (item === field) return true;
        }
        return false;
    }
    function available(field: Item): bool { return field !== null && field.visible && field.enabled; }
    function focusField(field: Item) {
        if (field instanceof SegmentedControl) (field as SegmentedControl).focusCurrent();
        else field.forceActiveFocus(Qt.TabFocusReason);
        Qt.callLater(revealCurrent);
    }
    function enter() {
        const field = available(memory.field) ? memory.field : fields.find(field => available(field));
        if (field) focusField(field);
    }
    function enterLast() {
        for (let index = fields.length - 1; index >= 0; --index) {
            if (available(fields[index])) { focusField(fields[index]); return; }
        }
    }
    function move(offset: int) {
        for (let index = currentIndex + offset; index >= 0 && index < fields.length; index += offset) {
            if (available(fields[index])) { focusField(fields[index]); return; }
        }
        boundaryReached(offset < 0 ? Qt.Key_Up : Qt.Key_Down);
    }
    function revealCurrent() {
        const flick = scrollView.contentItem as Flickable;
        if (currentIndex < 0 || !flick) return;
        const field = fields[currentIndex];
        const y = field.mapToItem(flick.contentItem, 0, 0).y;
        // Include the setting name above a slider or segmented choice.
        const top = Math.max(0, y - Theme.controlHeight);
        const bottom = y + field.height + Theme.spaceSm;
        const target = top < flick.contentY ? top
            : bottom > flick.contentY + flick.height ? bottom - flick.height : flick.contentY;
        flick.contentY = Math.max(0, Math.min(target, flick.contentHeight - flick.height));
    }
    onFocusItemChanged: {
        if (currentIndex < 0) return;
        memory.field = fields[currentIndex];
        Qt.callLater(revealKeyboardFocus);
    }
    function revealKeyboardFocus() { if (keyboardFocused) revealCurrent(); }
    readonly property Connections keys: Connections {
        target: root.currentIndex >= 0 ? root.focusItem.Keys : null
        function onUpPressed(event) {
            if ((event.modifiers & ~Qt.KeypadModifier) !== Qt.NoModifier) { event.accepted = false; return; }
            event.accepted = true;
            root.move(-1);
        }
        function onDownPressed(event) {
            if ((event.modifiers & ~Qt.KeypadModifier) !== Qt.NoModifier) { event.accepted = false; return; }
            event.accepted = true;
            root.move(1);
        }
    }
    readonly property Connections viewport: Connections {
        target: root.scrollView
        function onAvailableHeightChanged() { Qt.callLater(root.revealKeyboardFocus); }
    }
    readonly property Connections content: Connections {
        target: root.scrollView.contentItem as Flickable
        function onContentHeightChanged() { Qt.callLater(root.revealKeyboardFocus); }
    }
}
