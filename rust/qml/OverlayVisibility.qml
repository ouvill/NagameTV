import QtQuick

// UI-only inactivity state. The native window observer supplies activity without
// grabbing input from buttons, sliders, or scrolling views.
Item {
    id: overlay
    enum PointerLocation { Inside, Outside }
    property bool playing: false
    property bool pinned: false
    property int hideDelay: 3200
    property bool controlsVisible: true
    readonly property bool mayHide: enabled && playing && !pinned
    // Keep the window exit across the subsequent hover/popup unpin notifications.
    QtObject {
        id: pointer
        property int location: OverlayVisibility.Inside
    }

    function reveal() {
        controlsVisible = true;
        if (mayHide)
            timeout.restart();
        else
            timeout.stop();
    }
    function syncVisibility() {
        if (mayHide && pointer.location === OverlayVisibility.Outside) {
            timeout.stop();
            controlsVisible = false;
        } else {
            reveal();
        }
    }
    function pointerActivity() {
        pointer.location = OverlayVisibility.Inside;
        reveal();
    }
    function pointerExited() {
        pointer.location = OverlayVisibility.Outside;
        syncVisibility();
    }
    onMayHideChanged: syncVisibility()
    Timer {
        id: timeout
        interval: overlay.hideDelay
        repeat: false
        onTriggered: if (overlay.mayHide)
            overlay.controlsVisible = false
    }
    Component.onCompleted: reveal()
}
