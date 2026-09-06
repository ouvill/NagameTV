import QtQuick
import QtTest
import ".." as Viewer

TestCase {
    id: testCase
    name: "AnimatedPanel"
    when: windowShown
    visible: true
    width: 400
    height: 340
    property int created: 0
    property int released: 0
    Component {
        id: component
        Viewer.AnimatedPanel {
            width: 400
            height: 304
            sourceComponent: Rectangle {
                color: "#26302a"
                Component.onCompleted: testCase.created++
                Component.onDestruction: testCase.released++
            }
        }
    }
    property var panel
    function init() {
        failOnWarning(/.*/);
        created = 0;
        released = 0;
        panel = createTemporaryObject(component, testCase);
        verify(panel !== null);
    }
    function test_close_keeps_content_until_fade_finishes() {
        compare(panel.item, null);
        panel.open = true;
        tryCompare(panel, "opacity", 1);
        compare(created, 1);
        const original = panel.item;
        panel.open = false;
        compare(panel.enabled, false);
        compare(panel.item, original);
        compare(released, 0);
        tryCompare(panel, "active", false);
        compare(panel.item, null);
        tryCompare(testCase, "released", 1);
    }
    function test_reopen_during_close_keeps_one_instance() {
        panel.open = true;
        tryCompare(panel, "opacity", 1);
        const original = panel.item;
        panel.open = false;
        wait(40);
        verify(panel.opacity > 0 && panel.opacity < 1);
        panel.open = true;
        tryCompare(panel, "opacity", 1);
        compare(panel.item, original);
        compare(created, 1);
        compare(released, 0);
    }
    function test_shutdown_unloads_without_waiting_for_animation() {
        panel.open = true;
        tryCompare(panel, "opacity", 1);
        panel.shuttingDown = true;
        compare(panel.active, false);
        compare(panel.item, null);
        tryCompare(testCase, "released", 1);
    }
}
