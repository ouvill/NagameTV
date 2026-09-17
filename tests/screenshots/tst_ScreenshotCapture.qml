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
    // This fixture supplies an immutable test image to the real save queue.
    // Native video capture itself is checked through the production Main.qml.
    property var capturedImage: null
    QtObject {
        id: bridge
        readonly property bool screenshot_busy: backend.screenshot_busy
        readonly property string screenshot_error: backend.screenshot_error
        signal screenshot_finished(url file)
        function capture_screenshot() { return backend.save_screenshot(testCase.capturedImage); }
        function poll_screenshot() { backend.poll_screenshot(); }
    }
    Connections {
        target: backend
        function onScreenshot_finished(file) { bridge.screenshot_finished(file); }
    }
    Rectangle {
        id: picture
        width: 160; height: 90
        color: "#ff0000"
        Rectangle { x: 20; y: 20; width: 40; height: 30; color: "#00ff00" }
    }
    Rectangle { width: 10; height: 10; color: "#0000ff" }
    ScreenshotCapture { id: capture; target: picture; available: true; backend: bridge }
    Image { id: savedImage; visible: false; cache: false }
    SignalSpy { id: saved; target: capture; signalName: "saved" }
    SignalSpy { id: failed; target: capture; signalName: "failed" }
    function init() {
        failOnWarning(/.*/);
        tryCompare(capture, "busy", false);
        backend.configure_screenshot_format("png");
        capture.enabled = true; capture.available = true;
        picture.color = "#ff0000";
        savedImage.source = ""; savedImage.visible = false;
        saved.clear(); failed.clear();
        verify(backend.configure_screenshot_directory(Qt.resolvedUrl(".")));
        capturedImage = null;
        waitForRendering(picture);
        verify(picture.grabToImage(function(result) { testCase.capturedImage = result.image; }));
        tryVerify(() => capturedImage !== null);
    }
    function cleanup() { tryCompare(capture, "busy", false); }
    function test_unavailable_and_parallel_requests() {
        capture.available = false; capture.capture();
        compare(capture.busy, false);
        capture.available = true;
        capture.capture();
        verify(capture.busy); verify(capture.canCapture);
        capture.capture(); capture.capture();
        tryCompare(saved, "count", 3);
        compare(capture.busy, false); compare(failed.count, 0);
        const paths = saved.signalArguments.map(args => args[0].toString());
        compare(new Set(paths).size, 3);
    }
    function test_accepted_saves_finish_after_controls_become_unavailable() {
        capture.capture(); capture.capture();
        capture.available = false; capture.enabled = false;
        capture.capture();
        tryCompare(saved, "count", 2);
        compare(failed.count, 0);
    }
    function test_image_and_settings_stay_fixed_while_saving() {
        verify(backend.configure_screenshot_directory(Qt.resolvedUrl(encodeURIComponent("撮影 #100%"))));
        capture.capture();
        picture.color = "#0000ff";
        verify(backend.configure_screenshot_format("webp"));
        verify(backend.configure_screenshot_directory(Qt.resolvedUrl("another")));
        capture.capture();
        tryCompare(saved, "count", 2);
        const paths = saved.signalArguments.map(args => args[0].toString());
        const first = paths.find(path => path.endsWith(".png"));
        verify(first !== undefined); verify(!first.includes("/another/"));
        verify(paths.some(path => path.includes("/another/") && path.endsWith(".webp")));
        savedImage.source = first;
        tryCompare(savedImage, "status", Image.Ready);
        savedImage.width = 160; savedImage.height = 90; savedImage.visible = true;
        waitForRendering(savedImage);
        const image = grabImage(savedImage);
        const sx = image.width / savedImage.width, sy = image.height / savedImage.height;
        compare(image.pixel(Math.floor(5 * sx), Math.floor(5 * sy)), Qt.rgba(1, 0, 0, 1));
        compare(image.pixel(Math.floor(30 * sx), Math.floor(30 * sy)), Qt.rgba(0, 1, 0, 1));
    }
    function test_all_formats_decode_data() {
        return [{tag: "png", format: "png"}, {tag: "jpg", format: "jpg"}, {tag: "webp", format: "webp"}];
    }
    function test_all_formats_decode(data) {
        verify(backend.configure_screenshot_format(data.format));
        capture.capture();
        tryCompare(saved, "count", 1);
        const file = saved.signalArguments[0][0];
        verify(file.toString().endsWith("." + data.format));
        savedImage.source = file;
        tryCompare(savedImage, "status", Image.Ready);
        compare(savedImage.sourceSize.width, picture.width); compare(savedImage.sourceSize.height, picture.height);
        compare(failed.count, 0);
    }
    function test_unwritable_folder_does_not_replace_working_setting() {
        const previous = backend.screenshot_directory;
        verify(!backend.configure_screenshot_directory("file:///proc/mirakurun-screenshots"));
        compare(backend.screenshot_directory, previous); verify(backend.screenshot_error.length > 0);
        capture.capture();
        tryCompare(saved, "count", 1);
        compare(backend.screenshot_error, "");
    }
}
