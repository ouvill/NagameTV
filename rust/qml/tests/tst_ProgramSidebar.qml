import QtQuick
import QtTest
import QtQuick.Controls
import ".."

Item {
    ApplicationWindow {
        id: host
        visible: true
        width: 640
        height: 480
        TestCase {
            id: testCase
            name: "ProgramSidebar"
            when: windowShown
            width: 640
            height: 480
            visible: true
            property string programData: "null"
            SidePanel {
                id: panel
                width: 360
                sourceComponent: ProgramSidebar {
                    targetWindow: host
                    programJson: testCase.programData
                    iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
                }
            }
            function init() {
                failOnWarning(/.*/);
            }
            function test_updates_and_reverse_animation_keep_content_then_release() {
                compare(panel.item, null);
                testCase.width = 660;
                compare(panel.item, null);
                testCase.width = 640;
                testCase.programData = JSON.stringify({
                    name: "First",
                    description: "<b>plain</b>",
                    startAt: 0,
                    duration: 3600000
                });
                panel.open = true;
                tryVerify(() => panel.item !== null);
                tryCompare(panel, "x", 280);
                const view = panel.item;
                view.page = ProgramSidebar.Comments;
                const title = findChild(view, "commentProgramTitle");
                verify(title.visible);
                compare(title.text, "Program title unavailable");
                view.commentProgramTitle = "<b>NX program</b>";
                compare(title.text, "<b>NX program</b>");
                compare(title.textFormat, Text.PlainText);
                view.commentProgramTitle = "Other station";
                compare(title.text, "Other station");
                view.commentProgramTitle = "";
                compare(title.text, "Program title unavailable");
                view.page = ProgramSidebar.Program;
                compare(findChild(view, "programDescription").textFormat, Text.PlainText);
                compare(findChild(view, "programDescription").text, "<b>plain</b>");
                testCase.programData = JSON.stringify({
                    name: "Second",
                    description: "new",
                    startAt: 0,
                    duration: 3600000
                });
                compare(findChild(view, "programTitle").text, "Second");
                panel.open = false;
                wait(40);
                compare(panel.item, view);
                panel.open = true;
                tryCompare(panel, "x", 280);
                compare(panel.item, view);
                panel.open = false;
                tryVerify(() => panel.item === null);
                panel.open = true;
                tryVerify(() => panel.item !== null);
                panel.shuttingDown = true;
                compare(panel.item, null);
            }
        }
    }
}
