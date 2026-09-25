import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer
import "../ui-gallery"

// Explicit review artifacts only; no claim of automated visual comparison.
TestCase {
    name: "CaptureControls"
    width: 1280
    height: 720
    visible: true
    when: windowShown
    Controls { id: gallery; anchors.fill: parent }
    function capture(name, includePopups = false) {
        wait(Theme.panelDuration);
        gallery.Window.window.update();
        verify(waitForRendering(gallery));
        const path = Qt.resolvedUrl("../../build/ui-review/" + name + ".png").toString().replace(/^file:\/\//, "");
        const snapshot = grabImage(includePopups ? gallery.Window.window.contentItem : gallery);
        // Qt throws on save failure; this method has no return value.
        snapshot.save(path);
    }
    function test_capture_review_images() {
        failOnWarning(/.*/);
        capture("controls-1280");
        width = 640; height = 360;
        capture("controls-640");
        width = 1280; height = 720;
        const button = findChild(gallery, "button_0_" + ActionButton.Secondary);
        button.forceActiveFocus(Qt.TabFocusReason);
        capture("controls-focus");
        mouseMove(button, button.width / 2, button.height / 2);
        tryCompare(button, "hovered", true);
        capture("controls-hover");
        const choice = findChild(gallery, "choice");
        mouseClick(choice);
        tryCompare(choice.popup, "opened", true);
        capture("controls-choice", true);
    }
}
