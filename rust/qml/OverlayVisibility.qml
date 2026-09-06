import QtQuick

// UI-only inactivity state. The native window observer supplies activity without
// grabbing input from buttons, sliders, or scrolling views.
Item {
    id: overlay
    property bool playing: false
    property bool pinned: false
    property int hideDelay: 3200
    property bool controlsVisible: true
    readonly property bool mayHide: enabled && playing && !pinned

    function reveal() {
        controlsVisible = true;
        if (mayHide)
            timeout.restart();
        else
            timeout.stop();
    }
    onMayHideChanged: reveal()
    Timer {
        id: timeout
        interval: overlay.hideDelay
        repeat: false
        onTriggered: if (overlay.mayHide)
            overlay.controlsVisible = false
    }
    Component.onCompleted: reveal()
}
