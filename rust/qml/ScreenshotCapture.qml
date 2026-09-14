pragma ComponentBehavior: Bound
import QtCore
import QtQuick
import QtQuick.Dialogs

Item {
    id: root
    required property Item target
    required property bool available
    enum Phase { Idle, Capturing, Choosing }
    readonly property int phase: pending.operation ? pending.operation.phase : ScreenshotCapture.Idle
    readonly property bool busy: phase !== ScreenshotCapture.Idle
    readonly property bool canCapture: enabled && available && !busy && target.visible && target.width > 0 && target.height > 0
    signal saved(url file)
    signal failed(string message)

    // A Choosing operation owns its completed grab. Cancelling drops that image;
    // identity checks prevent an old asynchronous callback from opening a dialog.
    QtObject {
        id: pending
        property var operation: null
    }
    function capture() {
        if (!canCapture) return;
        const request = { phase: ScreenshotCapture.Capturing };
        pending.operation = request;
        const started = target.grabToImage(function(result) {
            if (pending.operation !== request) return;
            pending.operation = { phase: ScreenshotCapture.Choosing, result: result };
            const folder = destination.currentFolder.toString().replace(/\/$/, "");
            destination.selectedFile = folder + "/Mirakurun-" + Qt.formatDateTime(new Date(), "yyyyMMdd-HHmmss-zzz") + ".png";
            destination.open();
        });
        if (!started) {
            pending.operation = null;
            failed(qsTranslate("Main", "Could not capture the picture. Try again while the video is playing."));
        }
    }
    function cancel() {
        pending.operation = null;
        destination.close();
    }
    onEnabledChanged: if (!enabled) cancel()
    onAvailableChanged: if (!available && phase === ScreenshotCapture.Capturing) cancel()

    FileDialog {
        id: destination
        objectName: "screenshotDestination"
        title: qsTranslate("Main", "Save screenshot")
        fileMode: FileDialog.SaveFile
        nameFilters: [qsTranslate("Main", "PNG images (*.png)")]
        defaultSuffix: "png"
        currentFolder: StandardPaths.writableLocation(StandardPaths.PicturesLocation)
        onAccepted: {
            if (root.phase !== ScreenshotCapture.Choosing) return;
            const capture = pending.operation;
            pending.operation = null;
            // Keep the URL typed: Qt handles local and portal paths, including
            // spaces and non-ASCII names, without manually decoding file URLs.
            if (capture.result.saveToFile(selectedFile))
                root.saved(selectedFile);
            else
                root.failed(qsTranslate("Main", "Could not save the screenshot. Check the file name and folder permissions, then try again."));
        }
        onRejected: pending.operation = null
    }
}
