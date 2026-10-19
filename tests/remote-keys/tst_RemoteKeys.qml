import QtQuick
import QtQuick.Window
import QtTest
import MinimalViewer

TestCase {
    id: tests
    name: "RemoteKeys"
    when: windowShown
    ShortcutKey { id: keys }
    Component {
        id: detachedItem
        Item {}
    }
    Component {
        id: targetWindow
        Window {
            id: windowRoot
            width: 320
            height: 200
            visible: true
            property alias target: receiver
            property var presses: []
            property var releases: []
            Item {
                id: receiver
                anchors.fill: parent
                focus: true
                Keys.onPressed: event => {
                    windowRoot.presses = windowRoot.presses.concat([event.key]);
                    tests.compare(event.modifiers, Qt.NoModifier);
                    tests.verify(!event.isAutoRepeat);
                    event.accepted = true;
                }
                Keys.onReleased: event => {
                    windowRoot.releases = windowRoot.releases.concat([event.key]);
                    event.accepted = true;
                }
            }
        }
    }
    function makeWindow() {
        const result = createTemporaryObject(targetWindow, null);
        verify(result !== null);
        result.requestActivate();
        tryCompare(result, "active", true);
        result.target.forceActiveFocus();
        tryCompare(result.target, "activeFocus", true);
        return result;
    }
    function test_supported_keys_use_focus_and_back_filter() {
        const target = makeWindow();
        const expected = [Qt.Key_Up, Qt.Key_Down, Qt.Key_Left, Qt.Key_Right, Qt.Key_Return];
        for (const key of expected) verify(keys.post_key(target.target, key));
        tryVerify(() => target.presses.length === expected.length);
        tryVerify(() => target.releases.length === expected.length);
        compare(target.presses, expected);
        compare(target.releases, expected);
        keys.bind_back_button(target.target, true);
        verify(keys.post_key(target.target, Qt.Key_Back));
        tryVerify(() => target.presses.length === expected.length + 1);
        compare(target.presses[expected.length], Qt.Key_Escape);
        compare(target.releases[expected.length], Qt.Key_Escape);
    }
    function test_rejects_unsupported_and_missing_window() {
        const target = makeWindow();
        verify(!keys.post_key(target.target, Qt.Key_A));
        verify(!keys.post_key(null, Qt.Key_Up));
        const item = createTemporaryObject(detachedItem, null);
        verify(!keys.post_key(item, Qt.Key_Up));
        wait(50);
        compare(target.presses.length, 0);
    }
    function test_inactive_window_does_not_steal_activation() {
        const target = makeWindow();
        const foreground = makeWindow();
        tryCompare(target, "active", false);
        verify(keys.post_key(target.target, Qt.Key_Up));
        verify(keys.post_key(target.target, Qt.Key_Return));
        verify(keys.post_key(target.target, Qt.Key_Back));
        wait(100);
        compare(target.active, false);
        compare(foreground.active, true);
        console.log("Inactive remote key delivery: presses=" + JSON.stringify(target.presses)
                    + " releases=" + JSON.stringify(target.releases));
    }
}
