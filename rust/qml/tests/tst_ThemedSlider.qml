import QtQuick
import QtTest
import ".."

TestCase {
    name: "ThemedSlider"
    width: 240
    height: 100
    when: windowShown
    ThemedSlider { id: slider; x: 20; y: 20; from: 0; to: 1; stepSize: 0.1 }
    SignalSpy { id: moved; target: slider; signalName: "moved" }
    function test_updates_do_not_emit_user_actions_and_keyboard_still_works() {
        failOnWarning(/.*/);
        slider.value = 0.4;
        moved.clear();
        slider.value = 0.5;
        compare(moved.count, 0);
        slider.forceActiveFocus();
        keyClick(Qt.Key_Right);
        compare(moved.count, 1);
        fuzzyCompare(slider.value, 0.6, 0.001);
        slider.subdued = true;
        keyClick(Qt.Key_Left);
        compare(moved.count, 2);
        fuzzyCompare(slider.value, 0.5, 0.001);
    }
}
