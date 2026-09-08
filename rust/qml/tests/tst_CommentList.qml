import QtQuick
import QtTest
import ".."

TestCase {
    id: testCase
    name: "CommentList"
    when: windowShown
    width: 380
    height: 320
    visible: true
    Component {
        id: fixture
        CommentList {
            width: 360
            height: 280
            commentsJson: "[]"
            status: "Waiting"
        }
    }
    function comments(start, end) {
        let result = [];
        for (let i = start; i < end; ++i)
            result.push({id: String(i), time: "12:34:56", text: i % 3 === 0 ? "Long comment\nSecond line\nThird line\nFourth line" : "Duplicate text", source: "NX"});
        return JSON.stringify(result);
    }
    function view() {
        const list = createTemporaryObject(fixture, testCase);
        verify(list !== null);
        waitForRendering(list);
        return list;
    }
    function init() { failOnWarning(/.*/); }
    function test_initial_history_before_viewport_is_sized() {
        const list = createTemporaryObject(fixture, testCase, {
            width: 0, height: 0, commentsJson: comments(0, 200)
        });
        verify(list !== null);
        wait(50);
        list.width = 360;
        list.height = 280;
        verify(waitForRendering(list));
        tryCompare(list, "followPending", false);
        tryCompare(list, "atYEnd", true);
        const last = list.itemAtIndex(list.count - 1);
        verify(last !== null);
        verify(last.y < list.contentY + list.height);
    }
    function test_follow_only_at_bottom() {
        const list = view();
        list.commentsJson = comments(0, 40);
        tryCompare(list, "atYEnd", true);
        list.commentsJson = comments(0, 41);
        tryCompare(list, "atYEnd", true);
        tryCompare(list, "followPending", false);
        list.positionViewAtIndex(12, ListView.Beginning);
        list.contentY += 15;
        waitForRendering(list);
        verify(!list.atYEnd);
        const y = list.contentY;
        list.commentsJson = comments(0, 45);
        waitForRendering(list);
        compare(list.contentY, y);
        verify(!list.atYEnd);
        list.positionViewAtEnd();
        list.commentsJson = comments(0, 46);
        tryCompare(list, "atYEnd", true);
        const bar = findChild(list, "commentScrollBar");
        verify(bar.visible);
        verify(bar.size < 1);
    }
    function test_wheel_scrolling_stops_following() {
        const list = view();
        list.commentsJson = comments(0, 60);
        tryCompare(list, "followPending", false);
        mouseWheel(list, 150, 100, 0, 480);
        tryVerify(() => !list.atYEnd);
        tryCompare(list, "moving", false);
        const y = list.contentY;
        list.commentsJson = comments(0, 61);
        waitForRendering(list);
        compare(list.contentY, y);
        verify(!list.atYEnd);
    }
    function test_eviction_preserves_comment_and_offset() {
        const list = view();
        list.commentsJson = comments(0, 200);
        tryCompare(list, "followPending", false);
        list.positionViewAtIndex(85, ListView.Beginning);
        list.contentY += 15;
        waitForRendering(list);
        const index = list.indexAt(1, list.contentY + 1);
        const id = list.model.get(index).commentId;
        const offset = list.contentY - list.itemAtIndex(index).y;
        list.commentsJson = comments(10, 210);
        waitForRendering(list);
        const updated = list.indexAt(1, list.contentY + 1);
        compare(list.model.get(updated).commentId, id);
        fuzzyCompare(list.contentY - list.itemAtIndex(updated).y, offset, 1);
        verify(!list.atYEnd);
        compare(list.count, 200);
    }
    function test_evicted_anchor_stays_at_oldest_and_clear_resets() {
        const list = view();
        list.commentsJson = comments(0, 200);
        tryCompare(list, "followPending", false);
        list.positionViewAtBeginning();
        list.commentsJson = comments(10, 210);
        tryCompare(list, "atYBeginning", true);
        verify(!list.atYEnd);
        list.commentsJson = "[]";
        compare(list.count, 0);
        list.commentsJson = comments(400, 450);
        tryCompare(list, "atYEnd", true);
    }
}
