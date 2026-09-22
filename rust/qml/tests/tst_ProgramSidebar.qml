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
                channelModel: ChannelFixture {}
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
            function verifyPlaybackPage(view) {
                const list = findChild(view, "sidebarCommentList");
                const controls = findChild(view, "sidebarPlaybackSettings");
                compare(list.active, false);
                compare(list.item, null);
                verify(controls.visible);
                compare(controls.danmakuEnabled, view.danmakuEnabled);
                compare(controls.commentsEnabled, view.commentsEnabled);
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
                view.page = ProgramSidebar.Playback;
                view.commentStatus = "Waiting";
                verifyPlaybackPage(view);
                view.danmakuEnabled = true;
                verifyPlaybackPage(view);
                view.commentStatus = "<b>Connected</b>";
                verifyPlaybackPage(view);
                view.danmakuEnabled = false;
                verifyPlaybackPage(view);
                view.commentsEnabled = true;
                comments.append_test("Received while danmaku is off", 0, false);
                verifyPlaybackPage(view);
                view.commentsEnabled = false;
                verifyPlaybackPage(view);
                view.page = ProgramSidebar.Program;
                const list = findChild(view, "sidebarCommentList");
                compare(list.active, buildConfiguration.evaluation_comment_list);
                if (buildConfiguration.evaluation_comment_list) {
                    tryVerify(() => list.item !== null);
                    compare(list.item.commentModel, comments);
                    compare(list.item.count, comments.count);
                } else {
                    compare(list.source.toString(), "");
                    compare(list.item, null);
                }
                const status = findChild(view, "sidebarCommentStatus");
                compare(status.text, "NX-Jikkyo · " + view.commentStatus);
                compare(status.textFormat, Text.PlainText);
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
                view.recording = true;
                verify(!findChild(view, "commentProgramCard").visible);
                view.recording = false;
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
                view.page = ProgramSidebar.Playback;
                verifyPlaybackPage(view);
                panel.open = false;
                tryVerify(() => panel.item === null);
                panel.open = true;
                tryVerify(() => panel.item !== null);
                panel.item.page = ProgramSidebar.Playback;
                verifyPlaybackPage(panel.item);
                panel.shuttingDown = true;
                compare(panel.item, null);
            }
        }
    }
}
