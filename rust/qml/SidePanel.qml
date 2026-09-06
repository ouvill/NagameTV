import QtQuick

Loader {
    id: panel
    property bool open: false
    property bool shuttingDown: false
    property real reveal: open && !shuttingDown ? 1 : 0
    x: parent.width - width * reveal
    height: parent.height
    active: !shuttingDown && (open || reveal > 0)
    visible: active
    enabled: open && !shuttingDown
    Behavior on reveal {
        NumberAnimation {
            duration: 220
            easing.type: Easing.OutCubic
        }
    }
}
