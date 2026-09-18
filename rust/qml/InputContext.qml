import QtQuick
import QtQuick.Controls

Item {
    id: root
    enum Scope { Window, Navigation, Playback, Seek, Dismiss }
    enum FocusKind { Other, Text, Slider }
    required property Window targetWindow
    property bool playbackControls: false
    property bool guideVisible: false
    property bool channelsVisible: false
    readonly property Item focusItem: targetWindow ? targetWindow.activeFocusItem : null
    readonly property int focusKind: focusItem instanceof TextInput || focusItem instanceof TextEdit
        ? InputContext.Text : focusItem instanceof Slider ? InputContext.Slider : InputContext.Other
    readonly property bool editingText: focusKind === InputContext.Text
    // A closed Drawer keeps Overlay visible for edge dragging.
    readonly property bool popupOpen: Overlay.overlay ? Overlay.overlay.children.some(item => item.visible) : false
    readonly property bool viewing: !guideVisible && !channelsVisible
    readonly property bool navigationEnabled: enabled && !editingText && !popupOpen

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
