import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer

TestCase {
    id: testCase
    name: "SharedControls"
    width: 1280
    height: 720
    visible: true
    when: windowShown
    Controls { id: gallery; anchors.fill: parent }
    SignalSpy { id: clicked; signalName: "clicked" }
    function init() {
        failOnWarning(/.*/);
        width = 1280; height = 720;
        clicked.target = findChild(gallery, "button_0_" + ActionButton.Secondary);
        clicked.clear();
        findChild(gallery, "toggle_0").checked = false;
        findChild(gallery, "slider").value = 50;
        testCase.forceActiveFocus();
        mouseMove(testCase, width - 4, height - 4);
        gallery.Window.window.update();
        verify(waitForRendering(gallery));
    }
    function test_press_release_keeps_hit_area_and_invokes_once() {
        const button = clicked.target;
        const origin = button.mapToItem(testCase, 0, 0);
        const size = Qt.size(button.width, button.height);
        mousePress(button, button.width - 2, button.height / 2);
        tryCompare(button, "feedbackScale", Theme.pressScale);
        compare(button.mapToItem(testCase, 0, 0), origin);
        compare(Qt.size(button.width, button.height), size);
        compare(clicked.count, 0);
        mouseRelease(button, button.width - 2, button.height / 2);
        compare(clicked.count, 1);
        tryCompare(button, "feedbackScale", 1);
        // A second press during the return animation still executes once.
        mouseClick(button);
        compare(clicked.count, 2);
    }
    function test_keyboard_and_disabled_actions() {
        const button = clicked.target;
        button.forceActiveFocus(Qt.TabFocusReason);
        keyClick(Qt.Key_Space);
        compare(clicked.count, 1);
        const disabled = findChild(gallery, "button_3_" + ActionButton.Secondary);
        clicked.target = disabled; clicked.clear();
        mouseClick(disabled);
        compare(clicked.count, 0);
    }
    function test_hover_and_choice_popup() {
        const button = clicked.target;
        mouseMove(button, button.width / 2, button.height / 2);
        tryCompare(button, "hovered", true);
        const choice = findChild(gallery, "choice");
        mouseClick(choice);
        tryCompare(choice.popup, "opened", true);
        keyClick(Qt.Key_Escape);
        tryCompare(choice.popup, "visible", false);
    }
    function test_form_keyboard_navigation() {
        const slider = findChild(gallery, "slider");
        slider.value = 50;
        slider.forceActiveFocus(Qt.TabFocusReason);
        keyClick(Qt.Key_Right);
        compare(slider.value, 60);
        slider.subdued = true;
        keyClick(Qt.Key_Left);
        compare(slider.value, 50);
        slider.subdued = false;
        const toggle = findChild(gallery, "toggle_0");
        toggle.checked = false;
        toggle.forceActiveFocus(Qt.TabFocusReason);
        keyClick(Qt.Key_Space);
        compare(toggle.checked, true);
    }
}
