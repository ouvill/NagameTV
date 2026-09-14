import QtQuick
import QtTest
import MinimalViewer
import "../../rust/qml"

TestCase {
    id: testCase
    name: "ScreenshotCapture"
    when: windowShown
    visible: true
    width: 640
    height: 480
    Player { id: backend }
    Rectangle {
        id: picture
        width: 160
        height: 90
        color: "#ff0000"
        Rectangle { x: 20; y: 20; width: 40; height: 30; color: "#00ff00" }
    }
    // Sibling controls overlap on screen but must not be captured.
    Rectangle { width: 10; height: 10; color: "#0000ff" }
    ScreenshotCapture { id: capture; target: picture; available: true; backend: backend }
    Image { id: savedImage; visible: false; cache: false }
    SignalSpy { id: saved; target: capture; signalName: "saved" }
    SignalSpy { id: failed; target: capture; signalName: "failed" }

    function init() {
        failOnWarning(/.*/);
        capture.cancel();
        capture.enabled = true;
        capture.available = true;
        picture.color = "#ff0000";
        savedImage.source = "";
        savedImage.visible = false;
        saved.clear();
        failed.clear();
        // The script runs a copy in a temporary directory and removes all PNGs.
        verify(backend.configure_screenshot_directory(Qt.resolvedUrl(".")));
    }
    function cleanup() { capture.cancel(); }
    function test_unavailable_and_duplicate_requests() {
        capture.available = false;
        capture.capture();
        compare(capture.busy, false);
        capture.available = true;
        waitForRendering(picture);
        capture.capture();
        compare(capture.busy, true);
        capture.capture();
        tryCompare(saved, "count", 1);
        compare(capture.busy, false);
        compare(failed.count, 0);
    }
    function test_cancel_discards_late_capture() {
        waitForRendering(picture);
        capture.capture();
        compare(capture.busy, true);
        capture.cancel();
        wait(100);
        compare(saved.count, 0);
        capture.capture();
        capture.enabled = false;
        wait(100);
        compare(capture.busy, false);
        compare(saved.count, 0);
        capture.enabled = true;
        capture.capture();
        tryCompare(saved, "count", 1);
    }
    function test_saves_immediately_and_repeated_captures_have_distinct_paths() {
        verify(backend.configure_screenshot_directory(Qt.resolvedUrl(encodeURIComponent("撮影 #100%"))));
        waitForRendering(picture);
        capture.capture();
        tryCompare(saved, "count", 1);
        compare(failed.count, 0);
        compare(capture.busy, false);
        const first = saved.signalArguments[0][0];
        capture.capture();
        tryCompare(saved, "count", 2);
        verify(first !== saved.signalArguments[1][0]);
        savedImage.source = first;
        tryCompare(savedImage, "status", Image.Ready);
        savedImage.width = 160;
        savedImage.height = 90;
        savedImage.visible = true;
        waitForRendering(savedImage);
        const image = grabImage(savedImage);
        const sx = image.width / savedImage.width;
        const sy = image.height / savedImage.height;
        compare(image.pixel(Math.floor(5 * sx), Math.floor(5 * sy)), Qt.rgba(1, 0, 0, 1));
        compare(image.pixel(Math.floor(30 * sx), Math.floor(30 * sy)), Qt.rgba(0, 1, 0, 1));
    }
    function test_unwritable_folder_does_not_replace_working_setting() {
        const previous = backend.screenshot_directory;
        verify(!backend.configure_screenshot_directory("file:///proc/mirakurun-screenshots"));
        compare(backend.screenshot_directory, previous);
        verify(backend.screenshot_error.length > 0);
        capture.capture();
        tryCompare(saved, "count", 1);
        compare(backend.screenshot_error, "");
    }
}
