import QtQuick
import QtTest
import ".." as Viewer

TestCase {
    name: "DanmakuTimeline"
    when: windowShown
    visible: true
    width: 640
    height: 480
    Component {
        id: overlayComponent
        Viewer.DanmakuOverlay { width: 640; height: 480; fontSize: 21; textOpacity: 1; speed: 1 }
    }
    Component { id: timelineComponent; Viewer.DanmakuTimeline {} }
    property var overlay
    property var timeline
    function entries() { return Array.from(overlay.visuals.values()); }
    function initTestCase() { failOnWarning(/.*/); }
    function init() {
        overlay = createTemporaryObject(overlayComponent, this);
        timeline = createTemporaryObject(timelineComponent, this, {overlay: overlay});
        verify(timeline !== null);
    }
    function test_rust_schedule_and_seek_render_in_qml() {
        verify(timeline.load(JSON.stringify([{time: 3, text: "later", type: "top"}, {time: 1, text: "first"}, {time: 3, text: "same time", type: "bottom"}])));
        timeline.position = 1;
        compare(overlay.activeCount, 0);
        timeline.position = 1.01;
        compare(overlay.activeCount, 1);
        compare(entries()[0].text, "first");
        timeline.position = 2;
        compare(overlay.activeCount, 1);
        timeline.seek(3);
        timeline.position = 3;
        compare(overlay.activeCount, 0);
        timeline.position = 3.01;
        compare(overlay.activeCount, 2);
        compare(entries()[0].text, "later");
        timeline.position = 0;
        compare(overlay.activeCount, 0);
        timeline.position = 1.01;
        compare(entries()[0].text, "first");
        timeline.seek(100);
        timeline.position = 100;
        compare(overlay.activeCount, 0);
    }
    function test_pause_and_invalid_load_preserve_records() {
        verify(timeline.load('[{"time":0,"text":"zero"}]'));
        timeline.playing = false;
        timeline.position = 0.1;
        compare(overlay.activeCount, 0);
        timeline.playing = true;
        compare(entries()[0].text, "zero");
        verify(!timeline.load('[{"time":-1,"text":"invalid"}]'));
        compare(overlay.controller.timeline_count, 1);
        verify(overlay.controller.error.length > 0);
        verify(timeline.load('[{"time":2,"text":"replacement"}]'));
        compare(overlay.activeCount, 0);
        timeline.position = 2.1;
        compare(entries()[0].text, "replacement");
        timeline.clear();
        compare(overlay.controller.timeline_count, 0);
        compare(overlay.activeCount, 0);
    }
    function test_hidden_timeline_consumes_without_creating_labels() {
        verify(timeline.load('[{"time":1,"text":"hidden"},{"time":3,"text":"visible"}]'));
        overlay.visible = false;
        timeline.position = 2;
        compare(overlay.activeCount, 0);
        compare(entries().length, 0);
        overlay.visible = true;
        timeline.position = 4;
        compare(overlay.activeCount, 1);
        compare(entries()[0].text, "visible");
    }

}
