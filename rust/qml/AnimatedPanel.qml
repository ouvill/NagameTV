import QtQuick

// The visible lifetime owns the loaded subtree, including interrupted closes.
Loader {
    id: panel
    property bool open: false
    property bool shuttingDown: false
    active: !shuttingDown && (open || opacity > 0)
    visible: active
    enabled: open && !shuttingDown
    focus: enabled
    opacity: open && !shuttingDown ? 1 : 0
    Behavior on opacity {
        enabled: !panel.shuttingDown
        NumberAnimation {
            duration: 160
            easing.type: Easing.OutCubic
        }
    }
    transform: Translate {
        y: panel.open && !panel.shuttingDown ? 0 : panel.height
        Behavior on y {
            enabled: !panel.shuttingDown
            NumberAnimation {
                duration: 240
                easing.type: Easing.OutCubic
            }
        }
    }
}
