import QtQuick
import QtQuick.Controls

Item {
    id: actions
    required property Window targetWindow
    property bool guideEnabled: false
    property int restoreVisibility: Window.Windowed
    readonly property bool fullscreen: targetWindow !== null && targetWindow.visibility === Window.FullScreen
    // A closed Drawer keeps Overlay itself visible for edge-drag handling.
    // Only displayed overlay children should suspend navigation/inactivity.
    readonly property bool popupOpen: Overlay.overlay
        ? Overlay.overlay.children.some(item => item.visible) : false
    readonly property bool editingText: targetWindow !== null && (targetWindow.activeFocusItem instanceof TextInput || targetWindow.activeFocusItem instanceof TextEdit)
    readonly property bool navigationEnabled: enabled && !editingText && !popupOpen
    signal channelsToggleRequested
    signal guideToggleRequested
    signal channelStepRequested(int offset)
    signal escapeRequested

    function toggleFullscreen() {
        if (!targetWindow)
            return;
        if (fullscreen)
            leaveFullscreen();
        else {
            restoreVisibility = targetWindow.visibility === Window.Maximized ? Window.Maximized : Window.Windowed;
            targetWindow.showFullScreen();
        }
    }
    function leaveFullscreen() {
        if (!fullscreen)
            return;
        if (restoreVisibility === Window.Maximized)
            targetWindow.showMaximized();
        else
            targetWindow.showNormal();
    }
    Shortcut {
        sequence: "F11"
        context: Qt.WindowShortcut
        autoRepeat: false
        enabled: actions.enabled
        onActivated: actions.toggleFullscreen()
    }
    Shortcut {
        sequence: "C"
        context: Qt.WindowShortcut
        autoRepeat: false
        enabled: actions.navigationEnabled
        onActivated: actions.channelsToggleRequested()
    }
    Shortcut {
        sequence: "G"
        context: Qt.WindowShortcut
        autoRepeat: false
        enabled: actions.navigationEnabled && actions.guideEnabled
        onActivated: actions.guideToggleRequested()
    }
    Shortcut {
        sequence: "PgUp"
        context: Qt.WindowShortcut
        autoRepeat: false
        enabled: actions.navigationEnabled
        onActivated: actions.channelStepRequested(-1)
    }
    Shortcut {
        sequence: "PgDown"
        context: Qt.WindowShortcut
        autoRepeat: false
        enabled: actions.navigationEnabled
        onActivated: actions.channelStepRequested(1)
    }
    Shortcut {
        sequence: "Escape"
        context: Qt.WindowShortcut
        autoRepeat: false
        // Popup owns Escape first (including ComboBox menus and program details).
        enabled: actions.enabled && !actions.popupOpen
        onActivated: actions.escapeRequested()
    }
}
