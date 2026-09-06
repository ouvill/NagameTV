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
    SignalSpy { id: details; signalName: "detailsRequested" }
    function test_title_requests_details_without_owning_a_popup() {
        view.programJson = program("First", "Description");
        details.target = view;
        details.clear();
        verify(waitForRendering(view));
        mouseClick(findChild(view, "currentProgramButton"));
        compare(details.count, 1);
        view.programJson = program("Second", "Next program");
        compare(findChild(view, "currentProgramButton").text, "Second");
        compare(details.count, 1);
    }
}
