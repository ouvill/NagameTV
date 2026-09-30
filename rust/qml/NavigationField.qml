pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer

// Directional entry selects the field; pointer/Tab entry edits immediately.
SettingsField {
    id: control
    enum Interaction { Navigating, Editing }
    property int interaction: NavigationField.Navigating
    property bool editingEnabled: true
    signal navigationRequested(int key)
    readOnly: !editingEnabled || interaction === NavigationField.Navigating
    onEditingEnabledChanged: if (!editingEnabled) finishEditing()
    function focusForNavigation() {
        forceActiveFocus(Qt.OtherFocusReason);
        finishEditing();
    }
    function finishEditing() {
        interaction = NavigationField.Navigating;
        deselect();
    }
    function focusForEditing() {
        forceActiveFocus(Qt.OtherFocusReason);
        beginEditing();
    }
    function beginEditing() { if (editingEnabled) interaction = NavigationField.Editing; }
    function editOnFocus() {
        if ([Qt.MouseFocusReason, Qt.TabFocusReason, Qt.BacktabFocusReason].includes(focusReason)) beginEditing();
    }
    onActiveFocusChanged: { if (activeFocus) editOnFocus(); else finishEditing(); }
    onFocusReasonChanged: if (activeFocus) editOnFocus()
    TapHandler { onPressedChanged: if (pressed) control.beginEditing() }
    function confirm(event) {
        event.accepted = true;
        if (event.isAutoRepeat) return;
        if (preeditText.length > 0) { InputMethod.commit(); return; }
        if (interaction === NavigationField.Navigating) beginEditing();
        else event.accepted = false; // Native acceptance and IME confirmation.
    }
    function back(event) {
        event.accepted = interaction === NavigationField.Editing;
        if (!event.accepted || event.isAutoRepeat) return;
        if (preeditText.length > 0) InputMethod.reset();
        else finishEditing();
    }
    Keys.onReturnPressed: function(event) { confirm(event); }
    Keys.onEnterPressed: function(event) { confirm(event); }
    Keys.onEscapePressed: function(event) { back(event); }
    Keys.onBackPressed: function(event) { back(event); }
    Keys.onPressed: function(event) {
        if (interaction !== NavigationField.Navigating) return;
        if ((event.modifiers & ~Qt.KeypadModifier) !== Qt.NoModifier) return;
        if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key)) {
            event.accepted = true;
            navigationRequested(event.key);
        }
    }
}
