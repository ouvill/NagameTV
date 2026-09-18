import QtQuick
import QtTest
import QtQuick.Controls
import MinimalViewer 1.0

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
            TestBuildConfiguration { id: buildConfiguration }
            CommentModel { id: comments }
            SidePanel {
                id: panel
                width: 360
                sourceComponent: ProgramSidebar {
                    targetWindow: host
                    programJson: testCase.programData
                    commentModel: comments
                    evaluationCommentList: buildConfiguration.evaluation_comment_list
                    iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
                }
            }
            function init() {
                failOnWarning(/.*/);
            }
            function test_list_resource_is_evaluation_only() {
                const component = Qt.createComponent("qrc:/qt/qml/MinimalViewer/qml/CommentList.qml");
                compare(component.status, buildConfiguration.evaluation_comment_list ? Component.Ready : Component.Error);
                if (!buildConfiguration.evaluation_comment_list)
                    verify(component.errorString().indexOf("No such file") >= 0, component.errorString());
                component.destroy();
            }
            function verifyCommentPage(view) {
                const list = findChild(view, "sidebarCommentList");
                const controls = findChild(view, "sidebarCommentControls");
                compare(list.active, buildConfiguration.evaluation_comment_list);
                compare(controls.visible, !buildConfiguration.evaluation_comment_list);
                if (buildConfiguration.evaluation_comment_list) {
                    tryVerify(() => list.item !== null);
                    compare(list.item.commentModel, comments);
                    compare(list.item.status, view.commentStatus);
                } else {
                    compare(list.source.toString(), "");
                    compare(list.item, null);
                    const status = findChild(view, "sidebarCommentStatus");
                    verify(status.visible);
                    verify(status.enabled);
                    compare(status.text, "Live comments: " + view.commentStatus);
                    compare(status.textFormat, Text.PlainText);
                }
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
                view.commentStatus = "Waiting";
                verifyCommentPage(view);
                view.danmakuEnabled = true;
                verifyCommentPage(view);
                view.commentStatus = "<b>Connected</b>";
                verifyCommentPage(view);
                view.danmakuEnabled = false;
                verifyCommentPage(view);
                view.commentsEnabled = true;
                comments.append_test("Received while danmaku is off", 0, false);
                verifyCommentPage(view);
                if (buildConfiguration.evaluation_comment_list)
                    compare(findChild(view, "sidebarCommentList").item.count, comments.count);
                view.commentsEnabled = false;
                verifyCommentPage(view);
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
                comments.append_test("Pending follow before page destruction", 0, false);
                view.page = ProgramSidebar.Program;
                compare(findChild(view, "sidebarCommentList").item, null);
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
                view.page = ProgramSidebar.Comments;
                verifyCommentPage(view);
                panel.open = false;
                tryVerify(() => panel.item === null);
                panel.open = true;
                tryVerify(() => panel.item !== null);
                panel.item.page = ProgramSidebar.Comments;
                verifyCommentPage(panel.item);
                panel.shuttingDown = true;
                compare(panel.item, null);
            }
        }
    }
}
