import QtQuick
import QtTest
import MinimalViewer

TestCase {
    name: "VolumeSlider"
    width: 260
    height: 100
    when: windowShown
    visible: true
    Component {
        id: component
        VolumeSlider { x: 20; y: 20; width: 200; stepSize: 0.1; value: 0.5 }
    }
    property var slider
    SignalSpy { id: adjusted; signalName: "volumeRequested" }
    SignalSpy { id: saved; signalName: "saveRequested" }
    function init() {
        failOnWarning(/.*/);
        slider = createTemporaryObject(component, this);
        verify(slider !== null);
        adjusted.target = slider;
        saved.target = slider;
        adjusted.clear();
        saved.clear();
    }
    function test_backend_update_is_not_a_user_command() {
        slider.value = 0.7;
        slider.subdued = true;
        wait(550);
        compare(adjusted.count, 0);
        compare(saved.count, 0);
    }
    function test_keyboard_burst_adjusts_immediately_and_saves_once() {
        slider.forceActiveFocus();
        keyClick(Qt.Key_Right);
        compare(adjusted.count, 1);
        fuzzyCompare(adjusted.signalArguments[0][0], 0.6, 0.001);
        wait(200);
        keyClick(Qt.Key_Right);
        compare(adjusted.count, 2);
        fuzzyCompare(adjusted.signalArguments[1][0], 0.7, 0.001);
        wait(250);
        compare(saved.count, 0);
        tryCompare(saved, "count", 1);
        wait(500);
        compare(saved.count, 1);
    }
    function test_drag_commits_on_release_without_delayed_duplicate() {
        mousePress(slider, 70, 14);
        mouseMove(slider, 160, 14);
        compare(slider.pressed, true);
        verify(adjusted.count > 0);
        wait(550);
        compare(saved.count, 0);
        mouseRelease(slider, 160, 14);
        compare(slider.pressed, false);
        compare(saved.count, 1);
        wait(500);
        compare(saved.count, 1);
    }
    function test_shutdown_suppresses_pending_ui_save() {
        slider.forceActiveFocus();
        keyClick(Qt.Key_Right);
        compare(adjusted.count, 1);
        slider.closing = true;
        wait(550);
        compare(saved.count, 0);
    }
}
