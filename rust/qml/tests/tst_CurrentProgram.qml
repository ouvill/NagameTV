import QtQuick
import QtTest
import ".." as Viewer

TestCase {
    id: testCase
    name: "CurrentProgram"
    when: windowShown
    visible: true
    width: 640
    height: 480
    Component {
        id: component
        Viewer.CurrentProgram {
            width: 600
            programJson: "null"
            channelLabel: "01   総合テレビ"
        }
    }
    property var view
    function initTestCase() {
        failOnWarning(/.*/);
    }
    function init() {
        failOnWarning(/.*/);
        view = createTemporaryObject(component, testCase);
        verify(view !== null);
    }
    function program(name, description) {
        return JSON.stringify({
            name: name,
            description: description,
            startAt: 1788681600000,
            duration: 1800000
        });
    }
    function test_missing_program_and_channel_identity() {
        const button = findChild(view, "currentProgramButton");
        compare(button.enabled, false);
        view.programJson = program(null, null);
        compare(button.enabled, true);
        compare(button.text, "番組名未取得");
        view.programJson = "null";
        compare(button.enabled, false);
    }
    function test_details_follow_updates_and_are_released_on_close() {
        view.programJson = program("First", "<b>Plain text</b>\nSecond line");
        const button = findChild(view, "currentProgramButton");
        verify(waitForRendering(view));
        mouseClick(button);
        const loader = findChild(view, "programDetailsLoader");
        verify(loader.item !== null);
        const popup = loader.item;
        tryCompare(popup, "opened", true);
        compare(findChild(popup, "programTitle").text, "First");
        compare(findChild(popup, "programDescription").text, "<b>Plain text</b>\nSecond line");
        compare(findChild(popup, "programDescription").textFormat, Text.PlainText);
        view.programJson = program("Second", "Next program");
        compare(findChild(popup, "programTitle").text, "Second");
        // Channel identity changes do not replace the popup or its widgets.
        view.channelLabel = "02   教育テレビ";
        compare(loader.item, popup);
        popup.forceActiveFocus();
        keyClick(Qt.Key_Escape);
        tryCompare(loader, "item", null);
        compare(view.showDetails, false);
    }
}
