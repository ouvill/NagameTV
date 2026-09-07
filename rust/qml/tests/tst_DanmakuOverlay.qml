import QtQuick
import QtQuick.Controls
import QtTest
import ".." as Viewer

TestCase {
    name: "DanmakuOverlay"
    when: windowShown
    visible: true
    width: 640
    height: 480
    Component {
        id: component
        Viewer.DanmakuOverlay {
            width: 640
            height: 480
            fontSize: 21
            textOpacity: 1
            speed: 2
        }
    }
    property var overlay
    function initTestCase() { failOnWarning(/.*/); }
    function init() {
        overlay = createTemporaryObject(component, this);
        verify(overlay !== null);
    }
    function test_burst_limit_clear_and_immediate_reuse() {
        for (let i = 0; i < 1000; ++i)
            overlay.receive("comment " + i);
        compare(overlay.liveEntries.length, 64);
        compare(overlay.children.length, 64);
        overlay.clearComments();
        compare(overlay.liveEntries.length, 0);
        verify(overlay.laneEntries.every(entry => entry === null));
        // New entries must survive deferred destruction of the previous batch.
        overlay.receive("new channel");
        compare(overlay.liveEntries.length, 1);
        tryVerify(() => overlay.children.length === 1);
        compare(overlay.liveEntries[0].text, "new channel");
        overlay.clearComments();
        overlay.clearComments();
        tryVerify(() => overlay.children.length === 0);
    }
    function test_hiding_releases_objects_and_ignores_new_comments() {
        overlay.receive("visible");
        overlay.visible = false;
        overlay.receive("hidden");
        compare(overlay.liveEntries.length, 0);
        verify(overlay.laneEntries.every(entry => entry === null));
        tryVerify(() => overlay.children.length === 0);
        overlay.visible = true;
        overlay.receive("<b>plain text</b>");
        compare(overlay.liveEntries.length, 1);
        compare(overlay.liveEntries[0].textFormat, Text.PlainText);
        compare(overlay.liveEntries[0].text, "<b>plain text</b>");
    }
    function test_animation_completion_releases_object_and_lane() {
        overlay.receive("test");
        compare(overlay.liveEntries.length, 1);
        tryVerify(() => overlay.liveEntries.length === 0, 7000);
        verify(overlay.laneEntries.every(entry => entry === null));
        tryVerify(() => overlay.children.length === 0);
    }
}
