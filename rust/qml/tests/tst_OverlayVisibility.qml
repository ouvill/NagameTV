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
    function test_window_exit_hides_immediately_and_reentry_restarts_timeout() {
        overlay.playing = true;
        overlay.pointerExited();
        compare(overlay.controlsVisible, false);
        overlay.pointerActivity();
        compare(overlay.controlsVisible, true);
        tryCompare(overlay, "controlsVisible", false);
    }
    function test_pin_released_after_window_exit_does_not_reveal_controls() {
        overlay.playing = true;
        overlay.pinned = true;
        overlay.pointerExited();
        compare(overlay.controlsVisible, true);
        // Qt delivers Leave to the native observer before clearing button hover.
        overlay.pinned = false;
        compare(overlay.controlsVisible, false);
        overlay.pinned = true;
        compare(overlay.controlsVisible, true);
        overlay.pointerActivity();
        overlay.pinned = false;
        compare(overlay.controlsVisible, true);
        tryCompare(overlay, "controlsVisible", false);
    }
    function test_stopped_playback_remains_visible_outside_window() {
        overlay.pointerExited();
        compare(overlay.controlsVisible, true);
        overlay.playing = true;
        compare(overlay.controlsVisible, false);
        overlay.playing = false;
        compare(overlay.controlsVisible, true);
    }
}
