pragma ComponentBehavior: Bound
import QtQuick

Item {
    id: root
    required property Item target
    required property bool available
    required property var backend
    readonly property bool busy: pending.operation !== null
    readonly property bool canCapture: enabled && available && !busy && target.visible && target.width > 0 && target.height > 0
    signal saved(url file)
    signal failed(string message)

    // The request's identity discards callbacks from a cancelled capture.
    QtObject {
        id: pending
        property var operation: null
    }
    function capture() {
        if (!canCapture) return;
        const request = {};
        pending.operation = request;
        const started = target.grabToImage(function(result) {
            if (pending.operation !== request) return;
            const file = backend.save_screenshot(result.image);
            pending.operation = null;
            if (file.toString().length)
                root.saved(file);
            else
                root.failed(backend.screenshot_error);
        });
        if (!started) {
            pending.operation = null;
            failed(qsTranslate("Main", "Could not capture the picture. Try again while the video is playing."));
        }
    }
    function cancel() { pending.operation = null; }
    onEnabledChanged: if (!enabled) cancel()
    onAvailableChanged: if (!available) cancel()
}
