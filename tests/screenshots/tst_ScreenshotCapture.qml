import QtQuick
import QtQuick.Dialogs
import QtTest
import "../../rust/qml"

TestCase {
    id: testCase
    name: "ScreenshotCapture"
    when: windowShown
    visible: true
    width: 640
    height: 480
    Rectangle {
        id: picture
        width: 160
        height: 90
        color: "#ff0000"
        Rectangle { x: 20; y: 20; width: 40; height: 30; color: "#00ff00" }
    }
    // A sibling control overlaps the picture on screen but must not be saved.
    Rectangle { x: 0; y: 0; width: 10; height: 10; color: "#0000ff" }
    ScreenshotCapture { id: capture; target: picture; available: true }
    Image { id: savedImage; visible: false; cache: false }
    SignalSpy { id: saved; target: capture; signalName: "saved" }
    SignalSpy { id: failed; target: capture; signalName: "failed" }
    property var dialog

    function init() {
        failOnWarning(/.*/);
        capture.cancel();
        capture.enabled = true;
        capture.available = true;
        picture.color = "#ff0000";
        savedImage.source = "";
        saved.clear();
        failed.clear();
        dialog = findChild(capture, "screenshotDestination");
        // Exercise the real Quick dialog deterministically, using a real GPU.
        dialog.options = FileDialog.DontUseNativeDialog;
        dialog.currentFolder = Qt.resolvedUrl(".");
    }
    function cleanup() { capture.cancel(); }
    function beginCapture() {
        waitForRendering(picture);
        capture.capture();
        tryCompare(capture, "phase", ScreenshotCapture.Choosing);
        tryCompare(dialog, "visible", true);
    }
    function test_unavailable_and_duplicate_requests() {
        capture.available = false;
        capture.capture();
        compare(capture.phase, ScreenshotCapture.Idle);
        capture.available = true;
        beginCapture();
        const name = dialog.selectedFile;
        capture.capture();
        compare(dialog.selectedFile, name);
        compare(capture.busy, true);
        dialog.reject();
        tryCompare(capture, "phase", ScreenshotCapture.Idle);
        compare(saved.count, 0);
        compare(failed.count, 0);
    }
    function test_cancel_discards_late_capture() {
        waitForRendering(picture);
        capture.capture();
        compare(capture.phase, ScreenshotCapture.Capturing);
        capture.cancel();
        wait(100);
        compare(capture.phase, ScreenshotCapture.Idle);
        compare(dialog.visible, false);
        beginCapture();
        capture.enabled = false;
        compare(capture.phase, ScreenshotCapture.Idle);
        compare(dialog.visible, false);
    }
    function test_save_retains_captured_picture_after_playback_stops() {
        beginCapture();
        capture.available = false;
        picture.color = "#ffffff";
        // The validating script copies this fixture to an isolated temporary
        // directory, so successful saves are removed along with the fixture.
        dialog.selectedFile = Qt.resolvedUrl(encodeURIComponent("撮影 #100%.png"));
        dialog.accept();
        tryCompare(saved, "count", 1);
        compare(failed.count, 0);
        compare(capture.busy, false);
        savedImage.source = saved.signalArguments[0][0];
        tryCompare(savedImage, "status", Image.Ready);
        savedImage.width = 160;
        savedImage.height = 90;
        savedImage.visible = true;
        waitForRendering(savedImage);
        const image = grabImage(savedImage);
        compare(image.pixel(5, 5), Qt.rgba(1, 0, 0, 1));
        compare(image.pixel(30, 30), Qt.rgba(0, 1, 0, 1));
        savedImage.visible = false;
    }
    function test_failed_save_releases_capture_and_allows_retry() {
        beginCapture();
        dialog.selectedFile = Qt.resolvedUrl("missing-directory/screenshot.png");
        // Emit the dialog's completion signal to exercise an IO failure after
        // selection; an interactive dialog normally blocks this invalid path.
        dialog.accepted();
        compare(failed.count, 1);
        compare(saved.count, 0);
        compare(capture.busy, false);
        dialog.close();
        beginCapture();
        dialog.reject();
        compare(capture.busy, false);
    }
}
