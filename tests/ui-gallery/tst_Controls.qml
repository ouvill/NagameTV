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
    QtObject { id: selection; property int index: 0 }
    SignalSpy { id: activated; signalName: "activated" }
    SignalSpy { id: accepted; signalName: "accepted" }
    SignalSpy { id: navigation; signalName: "navigationRequested" }
    TestInputMethod { id: inputEvents }
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
        const origin = button.mapToItem(testCase, 0, 0);
        const size = Qt.size(button.width, button.height);
        button.forceActiveFocus(Qt.TabFocusReason);
        const outline = findChild(button, "focusOutline");
        tryCompare(outline, "opacity", 1);
        tryVerify(function() { return button.feedbackScale > 1; });
        compare(button.mapToItem(testCase, 0, 0), origin);
        compare(Qt.size(button.width, button.height), size);
        keyPress(Qt.Key_Space);
        tryCompare(button, "feedbackScale", Theme.pressScale);
        keyRelease(Qt.Key_Space);
        compare(clicked.count, 1);
        tryVerify(function() { return button.feedbackScale > 1; });
        const selected = findChild(gallery, "button_2_" + ActionButton.Secondary);
        const selectedOutline = findChild(selected, "focusOutline");
        compare(selectedOutline.opacity, 0);
        selected.forceActiveFocus(Qt.TabFocusReason);
        tryCompare(selectedOutline, "opacity", 1);
        tryCompare(outline, "opacity", 0);
        verify(selected.selected);
        button.forceActiveFocus(Qt.TabFocusReason);
        tryCompare(selectedOutline, "opacity", 0);
        verify(selected.selected);
        const disabled = findChild(gallery, "button_3_" + ActionButton.Secondary);
        clicked.target = disabled; clicked.clear();
        mouseClick(disabled);
        compare(clicked.count, 0);
        compare(findChild(disabled, "focusOutline").opacity, 0);
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
    function test_choice_enter_and_external_selection_binding() {
        const choice = findChild(gallery, "choice");
        selection.index = 0;
        choice.currentIndex = Qt.binding(function() { return selection.index; });
        activated.target = choice; activated.clear();
        choice.forceActiveFocus(Qt.TabFocusReason);
        keyPress(Qt.Key_Return); tryCompare(choice.popup, "opened", true);
        verify(inputEvents.forward_key(Qt.Key_Return, Qt.NoModifier, "", true));
        verify(choice.popup.opened); compare(activated.count, 0);
        keyRelease(Qt.Key_Return);
        keyClick(Qt.Key_Down); keyClick(Qt.Key_Return);
        tryCompare(choice.popup, "visible", false);
        compare(activated.count, 1); compare(choice.currentIndex, 1);
        selection.index = 1; selection.index = 0;
        compare(choice.currentIndex, 0);
        keyClick(Qt.Key_Enter); tryCompare(choice.popup, "opened", true);
        keyClick(Qt.Key_Escape); tryCompare(choice.popup, "visible", false);
        compare(activated.count, 1);
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
    function test_field_selection_editing_mouse_tab_and_ime() {
        const field = findChild(gallery, "navigationField");
        accepted.target = field; accepted.clear();
        navigation.target = field; navigation.clear();
        field.text = "draft";
        field.focusForNavigation();
        verify(field.readOnly); verify(!field.cursorVisible);
        keyClick(Qt.Key_A);
        compare(field.text, "draft");
        keyClick(Qt.Key_Right);
        compare(navigation.count, 1);
        keyPress(Qt.Key_Return);
        verify(!field.readOnly); verify(field.cursorVisible);
        verify(inputEvents.forward_key(Qt.Key_Return, Qt.NoModifier, "", true));
        keyRelease(Qt.Key_Return);
        compare(accepted.count, 0);
        field.cursorPosition = field.text.length;
        keyClick(Qt.Key_Left);
        compare(field.cursorPosition, field.text.length - 1);
        compare(navigation.count, 1);
        verify(inputEvents.compose("検索", ""));
        verify(field.inputMethodComposing);
        keyClick(Qt.Key_Escape);
        compare(field.interaction, NavigationField.Editing);
        verify(inputEvents.compose("", "検索"));
        keyClick(Qt.Key_Return);
        compare(accepted.count, 1);
        keyClick(Qt.Key_Escape);
        verify(field.readOnly); verify(!field.cursorVisible);
        verify(field.text.includes("検索"));
        mouseClick(field);
        verify(!field.readOnly);
        field.focusForNavigation();
        findChild(gallery, "field").forceActiveFocus(Qt.TabFocusReason);
        keyClick(Qt.Key_Tab);
        verify(field.activeFocus, "Tab focus: " + field.Window.window.activeFocusItem.objectName);
        verify(!field.readOnly);
    }
}
