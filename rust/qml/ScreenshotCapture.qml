pragma ComponentBehavior: Bound
import QtQuick

Item {
    id: root
    required property Item target
    required property bool available
    required property var backend
    readonly property bool busy: backend.screenshot_busy
    readonly property bool canCapture: enabled && available && target.visible && target.width > 0 && target.height > 0
    signal saved(url file)
    signal failed(string message)

    function capture() {
        if (canCapture && !backend.capture_screenshot()) failed(backend.screenshot_error);
    }
    Timer {
        interval: 16
        repeat: true
        running: root.backend.screenshot_busy
        onTriggered: root.backend.poll_screenshot()
    }
    Connections {
        target: root.backend
        function onScreenshot_finished(file) {
            if (file.toString().length) root.saved(file);
            else root.failed(root.backend.screenshot_error);
        }
    }
}
