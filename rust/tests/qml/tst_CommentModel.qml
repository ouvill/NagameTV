import QtQuick
import QtTest
import MinimalViewer

TestCase {
    name: "CommentModel"
    Component { id: factory; CommentModel {} }
    property var history
    SignalSpy { id: inserted; target: history; signalName: "rowsInserted" }
    SignalSpy { id: removed; target: history; signalName: "rowsRemoved" }
    SignalSpy { id: reset; target: history; signalName: "modelReset" }
    SignalSpy { id: updated; target: history; signalName: "updated" }
    function init() {
        failOnWarning(/.*/);
        history = createTemporaryObject(factory, this);
        history.enable_model_test();
        inserted.clear(); removed.clear(); reset.clear(); updated.clear();
    }
    function test_batches_notify_only_changed_rows() {
        history.append_batch_test(1999);
        compare(inserted.count, 1);
        compare(inserted.signalArguments[0][1], 0);
        compare(inserted.signalArguments[0][2], 1998);
        const retained = history.id_at(100);
        history.append_batch_test(11);
        compare(history.count, 2000);
        compare(removed.count, 1);
        compare(removed.signalArguments[0][1], 0);
        compare(removed.signalArguments[0][2], 9);
        compare(inserted.count, 2);
        compare(inserted.signalArguments[1][1], 1989);
        compare(inserted.signalArguments[1][2], 1999);
        compare(history.row_for_id(retained), 90);
        compare(reset.count, 0);
        compare(updated.count, 2);
        history.append_batch_test(0);
        compare(updated.count, 2);
    }
    function test_oversized_batch_and_clear_do_not_reuse_ids() {
        history.append_batch_test(2500);
        compare(history.count, 2000);
        compare(history.id_at(0), "500");
        compare(history.id_at(1999), "2499");
        const id = history.id_at(1999);
        history.clear_test();
        compare(history.count, 0);
        compare(reset.count, 0);
        compare(removed.count, 1);
        history.clear_test();
        compare(reset.count, 0);
        compare(removed.count, 1);
        history.append_test("new channel", 0, false);
        compare(history.id_at(0), "2500");
        compare(history.row_for_id(id), -1);
    }
}
