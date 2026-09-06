import QtQuick
import QtTest
import ".." as Viewer

TestCase {
    name: "OverlayVisibility"
    when: windowShown
    Component {
        id: component
        Viewer.OverlayVisibility {
            hideDelay: 60
        }
    }
    property var overlay
    function initTestCase() {
        failOnWarning(/.*/);
    }
    function init() {
        overlay = createTemporaryObject(component, this);
        verify(overlay !== null);
    }
    function test_play_stop_and_activity() {
        wait(100);
        compare(overlay.controlsVisible, true);
        overlay.playing = true;
        tryCompare(overlay, "controlsVisible", false);
        overlay.reveal();
        compare(overlay.controlsVisible, true);
        tryCompare(overlay, "controlsVisible", false);
        overlay.playing = false;
        compare(overlay.controlsVisible, true);
        wait(100);
        compare(overlay.controlsVisible, true);
    }
    function test_pin_cancels_deadline_and_release_restarts() {
        overlay.playing = true;
        overlay.pinned = true;
        wait(100);
        compare(overlay.controlsVisible, true);
        overlay.pinned = false;
        compare(overlay.controlsVisible, true);
        tryCompare(overlay, "controlsVisible", false);
        overlay.pinned = true;
        compare(overlay.controlsVisible, true);
    }
    function test_disable_cancels_timeout() {
        overlay.playing = true;
        overlay.enabled = false;
        wait(100);
        compare(overlay.controlsVisible, true);
    }
}
