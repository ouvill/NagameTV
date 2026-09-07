import QtQuick

// Reflect output immediately; persist once a pointer gesture or key burst ends.
// Backend value updates are presentation changes, not user commands.
ThemedSlider {
    id: slider
    property bool closing: false
    property bool keyboardGesture: false
    signal volumeRequested(real fraction)
    signal saveRequested
    from: 0
    to: 1
    Accessible.name: qsTranslate("Main", "Volume")
    // Slider.pressed also changes for arrow keys. Mark them before the control
    // handles the event so key release does not persist every repeat separately.
    Keys.onPressed: function (event) {
        if ([Qt.Key_Left, Qt.Key_Right, Qt.Key_Up, Qt.Key_Down].includes(event.key))
            keyboardGesture = true
        event.accepted = false
    }
    Keys.onReleased: function (event) {
        // A key ignored by the control must not mark a later pointer gesture.
        if (!pressed) keyboardGesture = false
        event.accepted = false
    }
    onMoved: {
        volumeRequested(value)
        if (!pressed) volumeSave.restart()
    }
    onPressedChanged: {
        volumeSave.stop()
        if (!pressed) {
            if (!closing) {
                if (keyboardGesture) volumeSave.restart()
                else saveRequested()
            }
            keyboardGesture = false
        }
    }
    Timer {
        id: volumeSave
        interval: 400
        // Keyboard and enabled wheel input need not change pressed.
        onTriggered: if (!slider.closing) slider.saveRequested()
    }
}
