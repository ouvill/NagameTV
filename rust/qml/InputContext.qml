import QtQuick
import QtQuick.Controls

Item {
    id: root
    enum Scope { Window, Navigation, Playback, Seek, Dismiss }
    enum FocusKind { Other, Text, Slider }
    required property Window targetWindow
    property bool playbackControls: false
    property bool guideVisible: false
    property bool libraryVisible: false
    property bool channelsVisible: false
    readonly property Item focusItem: targetWindow ? targetWindow.activeFocusItem : null
    readonly property int focusKind: focusItem instanceof TextInput || focusItem instanceof TextEdit
        ? InputContext.Text : focusItem instanceof Slider ? InputContext.Slider : InputContext.Other
    readonly property bool editingText: focusKind === InputContext.Text
    // A closed Drawer keeps Overlay visible for edge dragging.
    readonly property bool popupOpen: Overlay.overlay ? Overlay.overlay.children.some(item => item.visible) : false
    readonly property bool viewing: !guideVisible && !libraryVisible && !channelsVisible
    readonly property bool navigationEnabled: enabled && !libraryVisible && !editingText && !popupOpen

    // Qt Wayland can keep text-input enabled when focus moves from an editor
    // to a non-text item in the same window. Update after Qt has finished the
    // focus transition so the compositor stops sending those keys to the IME.
    // Query the actual focus object; the shortcut scope is not an IME policy.
    function updateInputMethod() {
        if (targetWindow && targetWindow.active)
            InputMethod.update(Qt.ImEnabled);
    }
    onFocusItemChanged: Qt.callLater(root.updateInputMethod)
    Component.onCompleted: Qt.callLater(root.updateInputMethod)
    Connections {
        target: root.targetWindow
        function onActiveChanged() { Qt.callLater(root.updateInputMethod); }
    }

    function accepts(scope) {
        if (!enabled) return false;
        switch (scope) {
        case InputContext.Window: return true;
        case InputContext.Navigation: return navigationEnabled;
        case InputContext.Playback: return navigationEnabled && playbackControls && viewing;
        case InputContext.Seek: return navigationEnabled && playbackControls && viewing && focusKind !== InputContext.Slider;
        case InputContext.Dismiss: return !popupOpen;
        default: throw new Error("Unknown shortcut scope: " + scope);
        }
    }
}
