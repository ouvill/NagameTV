import QtQuick
import QtQuick.Controls

Item {
    id: root
    enum Scope { Window, Navigation, Playback, Seek, Dismiss }
    enum FocusKind { Other, Text, Slider }
    required property Window targetWindow
    required property Item videoItem
    property bool playbackControls: false
    property bool guideVisible: false
    property bool libraryVisible: false
    property bool channelsVisible: false
    property bool dataBroadcastOpen: false
    property bool dataBroadcastFocused: false
    property list<string> dataBroadcastKeys: []
    readonly property Item focusItem: targetWindow ? targetWindow.activeFocusItem : null
    readonly property bool videoFocused: focusItem === videoItem
    readonly property int focusKind: focusItem instanceof TextInput || focusItem instanceof TextEdit
        ? InputContext.Text : focusItem instanceof Slider ? InputContext.Slider : InputContext.Other
    readonly property bool editingText: focusKind === InputContext.Text
    // Closed drawers and passive tooltips also use Overlay. Only a popup
    // owning keyboard focus takes input away from the underlying screen.
    readonly property bool popupOpen: Overlay.overlay ? Overlay.overlay.children.some(item => item.visible && item.activeFocus) : false
    readonly property bool viewing: !guideVisible && !libraryVisible && !channelsVisible
    readonly property bool receiverInputEnabled: enabled && !libraryVisible && !editingText && !popupOpen
    readonly property bool navigationEnabled: receiverInputEnabled && !bmlAccepts("Up")

    function bmlAccepts(sequence: string): bool {
        // A retained, hidden engine must leave receiver navigation usable,
        // even if its document still advertises basic keys.
        if (!enabled || !dataBroadcastOpen || !viewing || popupOpen || editingText) return false;
        let group = "";
        switch (sequence) {
        case "Up": case "Down": case "Left": case "Right":
        case "Return": case "Enter": case "Space": case "Back": case "Backspace": case "X":
            group = "basic"; break;
        case "B": case "R": case "G": case "Y": group = "data-button"; break;
        case "0": case "1": case "2": case "3": case "4": case "5": case "6": case "7": case "8": case "9":
            group = "numeric-tuning"; break;
        default: return false;
        }
        return dataBroadcastKeys.includes(group);
    }

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
        case InputContext.Navigation: return receiverInputEnabled;
        case InputContext.Playback: return receiverInputEnabled && playbackControls && viewing && (videoFocused || dataBroadcastFocused);
        case InputContext.Seek: return receiverInputEnabled && playbackControls && viewing && (videoFocused || dataBroadcastFocused);
        case InputContext.Dismiss: return !popupOpen && !(focusItem instanceof NavigationField
            && (focusItem as NavigationField).interaction === NavigationField.Editing);
        default: throw new Error("Unknown shortcut scope: " + scope);
        }
    }
}
