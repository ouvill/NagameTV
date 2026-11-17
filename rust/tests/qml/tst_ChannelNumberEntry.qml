import QtQuick
import QtTest
import MinimalViewer as Viewer

TestCase {
    id: testCase
    name: "ChannelNumberEntry"
    when: windowShown
    visible: true
    width: 320
    height: 160
    Component { id: component; Viewer.ChannelNumberEntry {} }
    property var entry
    SignalSpy { id: committed; signalName: "committed" }
    function init() {
        failOnWarning(/.*/);
        entry = createTemporaryObject(component, testCase);
        committed.target = entry;
        committed.clear();
    }
    function test_single_digits_tune_at_once() {
        entry.press(5);
        compare(committed.count, 1);
        compare(committed.signalArguments[0][0], 5);
        compare(findChild(entry, "channelNumberBadge").text, "5");
        entry.press(0);
        compare(committed.signalArguments[1][0], 10);
    }
    function test_a_leading_one_waits_for_a_second_digit() {
        entry.press(1);
        compare(committed.count, 0);
        compare(findChild(entry, "channelNumberBadge").text, "1–");
        entry.press(2);
        compare(committed.count, 1);
        compare(committed.signalArguments[0][0], 12);
        entry.press(1);
        entry.press(7);
        compare(committed.signalArguments[1][0], 7);
    }
    function test_a_lone_one_tunes_after_the_wait() {
        entry.press(1);
        tryCompare(committed, "count", 1, entry.secondDigitWait + 1000);
        compare(committed.signalArguments[0][0], 1);
    }
    function test_cancel_forgets_a_pending_digit() {
        entry.press(1);
        entry.cancel();
        wait(entry.secondDigitWait + 200);
        compare(committed.count, 0);
        compare(entry.digits, "");
    }
}
