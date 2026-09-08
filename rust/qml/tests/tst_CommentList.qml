import QtQuick
import QtTest
import MinimalViewer 1.0
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
            commentModel: CommentModel {}
            status: "Waiting"
        }
    }
    Component {
        id: loaderFixture
        Loader {
            width: 360
            height: 280
            sourceComponent: CommentList {
                commentModel: CommentModel {}
                status: "Waiting"
            }
        }
    }
    Component { id: modelFixture; CommentModel {} }
    function appendComments(model, start, end) {
        for (let i = start; i < end; ++i)
            model.append_test(i % 3 === 0 ? "Long comment\nSecond line\nThird line\nFourth line" : "Duplicate text", 0, false);
    }
    function view(initialCount = 0) {
        const model = createTemporaryObject(modelFixture, testCase);
        appendComments(model, 0, initialCount);
        const list = createTemporaryObject(fixture, testCase, {commentModel: model});
        verify(list !== null);
        waitForRendering(list);
        return list;
    }
    function init() { failOnWarning(/.*/); }
    function test_native_roles_and_model_replacement() {
        const list = view();
        list.commentModel.append_test("<b>plain comment</b>", 0, false);
        tryCompare(list, "count", 1);
        list.forceLayout();
        const row = list.itemAtIndex(0);
        verify(row !== null);
        compare(row.text, "<b>plain comment</b>");
        verify(row.time.length > 0);
        verify(row.source.length > 0);
        const oldModel = list.commentModel;
        const replacement = createTemporaryObject(modelFixture, testCase);
        appendComments(replacement, 0, 40);
        list.commentModel = replacement;
        tryCompare(list, "count", 40);
        tryCompare(list, "followPending", false);
        tryCompare(list, "atYEnd", true);
        oldModel.clear_test();
        compare(list.count, 40);
    }
    function test_destroy_before_delayed_follow() {
        const loader = createTemporaryObject(loaderFixture, testCase);
        verify(loader.item !== null);
        appendComments(loader.item.commentModel, 0, 40);
        loader.active = false;
        compare(loader.item, null);
        wait(50);
    }
    function test_cancel_before_delayed_follow() {
        const list = view();
        appendComments(list.commentModel, 0, 40);
        list.cancelFollow();
        list.positionViewAtBeginning();
        const y = list.contentY;
        wait(50);
        compare(list.contentY, y);
        verify(!list.atYEnd);
    }
    function test_initial_history_before_viewport_is_sized() {
        const model = createTemporaryObject(modelFixture, testCase);
        appendComments(model, 0, 2000);
        const list = createTemporaryObject(fixture, testCase, {
            width: 0, height: 0, commentModel: model
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
        appendComments(list.commentModel, 0, 40);
        tryCompare(list, "atYEnd", true);
        appendComments(list.commentModel, 40, 41);
        tryCompare(list, "atYEnd", true);
        tryCompare(list, "followPending", false);
        list.positionViewAtIndex(12, ListView.Beginning);
        list.contentY += 15;
        waitForRendering(list);
        verify(!list.atYEnd);
        const y = list.contentY;
        appendComments(list.commentModel, 41, 45);
        waitForRendering(list);
        compare(list.contentY, y);
        verify(!list.atYEnd);
        list.positionViewAtEnd();
        appendComments(list.commentModel, 45, 46);
        tryCompare(list, "atYEnd", true);
        const bar = findChild(list, "commentScrollBar");
        verify(bar.visible);
        verify(bar.size < 1);
    }
    function test_wheel_scrolling_stops_following() {
        const list = view();
        appendComments(list.commentModel, 0, 60);
        tryCompare(list, "followPending", false);
        mouseWheel(list, 150, 100, 0, 480);
        tryVerify(() => !list.atYEnd);
        tryCompare(list, "moving", false);
        const y = list.contentY;
        appendComments(list.commentModel, 60, 61);
        waitForRendering(list);
        compare(list.contentY, y);
        verify(!list.atYEnd);
    }
    function test_eviction_preserves_comment_and_offset() {
        const list = view(2000);
        tryCompare(list, "followPending", false);
        list.positionViewAtIndex(85, ListView.Beginning);
        list.contentY += 15;
        waitForRendering(list);
        const index = list.indexAt(1, list.contentY + 1);
        const id = list.commentModel.id_at(index);
        const offset = list.contentY - list.itemAtIndex(index).y;
        appendComments(list.commentModel, 2000, 2010);
        waitForRendering(list);
        const updated = list.indexAt(1, list.contentY + 1);
        compare(list.commentModel.id_at(updated), id);
        fuzzyCompare(list.contentY - list.itemAtIndex(updated).y, offset, 1);
        verify(!list.atYEnd);
        compare(list.count, 2000);
    }
    function test_evicted_anchor_stays_at_oldest_and_clear_resets() {
        const list = view(2000);
        tryCompare(list, "followPending", false);
        list.positionViewAtBeginning();
        appendComments(list.commentModel, 2000, 2010);
        tryCompare(list, "atYBeginning", true);
        verify(!list.atYEnd);
        list.commentModel.clear_test();
        compare(list.count, 0);
        appendComments(list.commentModel, 400, 450);
        tryCompare(list, "atYEnd", true);
    }
}
