pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import QtQuick.Templates as Templates
import MinimalViewer

// A form is traversed by rows. Its controls keep their own horizontal value
// adjustment and confirmation; vertical keys never change a slider's value.
QtObject {
    id: root
    property ScrollView scrollView: null
    property Item focusScope: scrollView
    required property list<Item> fields
    readonly property Item focusItem: focusScope && focusScope.Window.window ? focusScope.Window.window.activeFocusItem : null
    readonly property Templates.Control focusedControl: focusItem as Templates.Control
    readonly property bool keyboardFocused: focusItem instanceof ActionButton
        ? (focusItem as ActionButton).focusVisible : focusItem instanceof ToggleSwitch
        ? (focusItem as ToggleSwitch).focusVisible : focusedControl !== null && focusedControl.visualFocus
    readonly property int currentIndex: fields.findIndex(field => containsFocus(field))
    readonly property Item currentField: currentIndex >= 0 ? fields[currentIndex] : null
    readonly property bool navigating: currentField !== null
        && (!(currentField instanceof SettingsChoice) || !(currentField as SettingsChoice).popup.visible)
        && (!(currentField instanceof NavigationField) || (currentField as NavigationField).interaction === NavigationField.Navigating)
        && (!(focusItem instanceof TextInput) || !(focusItem as TextInput).inputMethodComposing)
    signal boundaryReached(int key)
    readonly property QtObject focusMemory: QtObject { id: memory; property Item field: null }
    function reset() { memory.field = null; }
    function containsFocus(field: Item): bool {
        for (let item = focusItem; item; item = item.parent) {
            if (item === field) return true;
        }
        return false;
    }
    function available(field: Item): bool { return field !== null && field.visible && field.enabled; }
    function focusField(field: Item) {
        if (field instanceof SegmentedControl) (field as SegmentedControl).focusCurrent();
        else if (field instanceof ThemedSpinBox) (field as ThemedSpinBox).focusForNavigation();
        else if (field instanceof NavigationField) (field as NavigationField).focusForNavigation();
        else field.forceActiveFocus(Qt.TabFocusReason);
        Qt.callLater(revealCurrent);
    }
    function enter(): bool {
        const field = fields.includes(memory.field) && available(memory.field) ? memory.field : fields.find(field => available(field));
        if (!field) return false;
        focusField(field);
        return true;
    }
    function enterLast(): bool {
        for (let index = fields.length - 1; index >= 0; --index) {
            if (available(fields[index])) { focusField(fields[index]); return true; }
        }
        return false;
    }
    function move(offset: int) {
        for (let index = currentIndex + offset; index >= 0 && index < fields.length; index += offset) {
            if (available(fields[index])) { focusField(fields[index]); return; }
        }
        boundaryReached(offset < 0 ? Qt.Key_Up : Qt.Key_Down);
    }
    function revealCurrent() {
        const flick = scrollView ? scrollView.contentItem as Flickable : null;
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
    function horizontal(event, offset: int) {
        event.accepted = false;
        if (!navigating || (event.modifiers & ~Qt.KeypadModifier) !== Qt.NoModifier) return;
        if (currentField instanceof ThemedSpinBox) {
            const number = currentField as ThemedSpinBox;
            if (number.interaction !== ThemedSpinBox.Stepping) return;
            number.stepBy(offset);
        } else {
            if (currentField instanceof Slider || currentField instanceof SegmentedControl
                || currentField instanceof ComboBox || (focusItem instanceof TextInput && !(currentField instanceof NavigationField))
                || focusItem instanceof TextEdit) return;
            const next = fields[currentIndex + offset];
            const origin = currentField.mapToItem(null, 0, 0);
            const point = next ? next.mapToItem(null, 0, 0) : null;
            if (next && available(next) && Math.abs(point.y - origin.y) < currentField.height / 2)
                focusField(next);
            else boundaryReached(offset < 0 ? Qt.Key_Left : Qt.Key_Right);
        }
        event.accepted = true;
    }
    function confirm(event) {
        event.accepted = false;
        if (!navigating) return;
        if (currentField instanceof ThemedSpinBox) {
            event.accepted = true;
            if (!event.isAutoRepeat) (currentField as ThemedSpinBox).toggleEditing();
        }
    }
    readonly property Connections horizontalKeys: Connections {
        target: root.currentIndex >= 0 && (root.currentField instanceof ThemedSpinBox
            || root.currentField instanceof NavigationField
            || (!(root.currentField instanceof Slider) && !(root.currentField instanceof SegmentedControl)
                && !(root.currentField instanceof ComboBox) && !(root.focusItem instanceof TextInput)
                && !(root.focusItem instanceof TextEdit))) ? root.focusItem.Keys : null
        function onLeftPressed(event) { root.horizontal(event, -1); }
        function onRightPressed(event) { root.horizontal(event, 1); }
    }
    readonly property Connections confirmKeys: Connections {
        target: root.currentField instanceof ThemedSpinBox ? root.focusItem.Keys : null
        function onReturnPressed(event) { root.confirm(event); }
        function onEnterPressed(event) { root.confirm(event); }
    }
    readonly property Connections keys: Connections {
        // Keep handlers attached while a choice is open, and explicitly pass
        // its keys through. Detaching a handler can leave Keys accepting the
        // specific key without delivering it to the native control.
        target: root.currentIndex >= 0 ? root.focusItem.Keys : null
        function onUpPressed(event) {
            if (!root.navigating || (event.modifiers & ~Qt.KeypadModifier) !== Qt.NoModifier) { event.accepted = false; return; }
            event.accepted = true;
            root.move(-1);
        }
        function onDownPressed(event) {
            if (!root.navigating || (event.modifiers & ~Qt.KeypadModifier) !== Qt.NoModifier) { event.accepted = false; return; }
            event.accepted = true;
            root.move(1);
        }
    }
    readonly property Connections viewport: Connections {
        target: root.scrollView
        function onAvailableHeightChanged() { Qt.callLater(root.revealKeyboardFocus); }
    }
    readonly property Connections content: Connections {
        target: root.scrollView ? root.scrollView.contentItem as Flickable : null
        function onContentHeightChanged() { Qt.callLater(root.revealKeyboardFocus); }
    }
}
