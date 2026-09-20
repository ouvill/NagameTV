import QtQuick
import QtQuick.Controls
import QtTest
import ".." as Viewer

TestCase {
    name: "CommentPresentation"
    when: windowShown
    visible: true
    width: 640
    height: 480
    Component {
        id: component
        Viewer.DanmakuOverlay { width: 640; height: 480; fontSize: 21; textOpacity: 1; speed: 1 }
    }
    property var overlay
    function entries() { return Array.from(overlay.visuals.values()); }
    function initTestCase() { failOnWarning(/.*/); }
    function init() { overlay = createTemporaryObject(component, this); verify(overlay !== null); }
    function records() {
        return JSON.stringify([
            {id: "a", time: 0, text: "実況コメント あいう", type: "right", own: true},
            {id: "b", time: 0.5, text: "別のコメント", type: "top"},
            {id: "c", time: 1, text: "三つ目のコメント", type: "bottom"}
        ]);
    }
    function seek(position) { overlay.controller.seek(position); }
    function load() { verify(overlay.controller.load_timeline(records())); }
    function test_live_arrival_enters_at_right_edge_and_replay_restores_timestamp() {
        const receivedAt = 10;
        const postedAt = 8;
        const live = JSON.stringify([{id: "arrival", time: receivedAt, text: "遅れて届いた新着", timing: "live"}]);
        verify(overlay.controller.update_timeline(live, receivedAt + 0.5, true));
        compare(overlay.activeCount, 1);
        const entry = entries()[0];
        compare(entry.x, overlay.width);
        const fullDuration = entry.duration;
        verify(overlay.controller.update_timeline(live, receivedAt + 1, false));
        compare(entries()[0], entry);
        fuzzyCompare(entry.x, overlay.width - overlay.width * 0.5 / 5, 0.01);
        overlay.paused = true;
        const replay = JSON.stringify([{id: "stored", time: postedAt, text: "遅れて届いた新着"}]);
        verify(overlay.controller.update_timeline(replay, receivedAt, true));
        compare(overlay.activeCount, 1);
        fuzzyCompare(entries()[0].x, overlay.width - overlay.width * (receivedAt - postedAt) / 5, 0.01);
        verify(Math.abs(entries()[0].duration - (fullDuration - 2000)) <= 1);
    }
    function test_default_burst_budget_and_clear() {
        const burst = Array.from({length: 1000}, (_, i) => ({time: 0, text: "burst " + i}));
        verify(overlay.controller.load_timeline(JSON.stringify(burst)));
        seek(0);
        compare(overlay.activeCount, 1);
        compare(entries().length, 1);
        overlay.clearComments();
        compare(overlay.screenshotLayer(overlay).commands.length, 0);
        tryCompare(overlay, "visualCount", 0);
        overlay.controller.reset();
        verify(overlay.receive("new", "right", 0xffffff));
        compare(entries()[0].text, "new");
    }
    function test_timed_scroll_constant_velocity_pause_and_resize() {
        load();
        overlay.paused = true;
        seek(2);
        compare(overlay.activeCount, 3);
        const old = entries();
        const x = old[0].x;
        fuzzyCompare(x, 640 - 640 * 2 / 5, 0.01);
        wait(100);
        compare(old[0].x, x);
        overlay.width = 800;
        compare(old[0].x, x);
        compare(old[1].x, (800 - old[1].width) / 2);
        overlay.fontSize = 36;
        compare(entries()[0], old[0]);
        compare(old[0].x, x);
        compare(old[0].font.pixelSize, 36);
        seek(100);
        compare(overlay.activeCount, 0);
        tryCompare(overlay, "visualCount", 0);
    }
    function test_fountain_seek_restore_fade_and_snapshot_pose() {
        overlay.displayMode = "pop";
        overlay.placementMode = "random";
        overlay.paused = true;
        load();
        seek(2);
        compare(overlay.activeCount, 3);
        const old = entries();
        const poses = old.map(e => [e.x, e.y, e.rotation, e.opacity]);
        verify(poses.some(p => Math.abs(p[2]) > 1));
        for (const entry of old) {
            verify(Math.abs(entry.rotation) <= 20);
            verify(entry.y < overlay.height - entry.height);
            verify(entry.opacity > 0 && entry.opacity <= 1);
        }
        const commands = overlay.screenshotLayer(overlay).commands;
        const text = commands.filter(c => c.kind === "text");
        compare(text.length, 3);
        for (let i = 0; i < 3; ++i) {
            compare(text[i].pose.degrees, old[i].rotation);
            fuzzyCompare(text[i].pose.x, old[i].x + old[i].width / 2, 0.01);
        }
        wait(100);
        compare(entries().map(e => [e.x, e.y, e.rotation, e.opacity]), poses);
        seek(0); seek(2);
        compare(entries().map(e => [e.x, e.y, e.rotation, e.opacity]), poses);
        seek(3.5);
        verify(entries()[0].y > poses[0][1]);
        seek(10);
        compare(overlay.activeCount, 0);
    }
    function test_switch_mode_clears_old_labels_and_invalidates_measurements() {
        load(); seek(2);
        const previous = entries()[0];
        overlay.displayMode = "pop";
        compare(overlay.activeCount, 3);
        verify(entries()[0] !== previous);
        compare(overlay.screenshotLayer(overlay).commands.filter(c => c.kind === "text").length, 3);
        verify(Math.abs(entries()[0].rotation) <= 20);
        tryVerify(() => overlay.visualCount === overlay.activeCount);
        overlay.visible = false;
        compare(overlay.activeCount, 0);
        tryCompare(overlay, "visualCount", 0);
        overlay.visible = true;
        compare(overlay.activeCount, 0);
    }
    function test_fountain_direct_clock_pause_shadow_and_own_frame() {
        overlay.displayMode = "pop";
        verify(overlay.receive("飛び込むコメント", "top", 0xffffff, true));
        const entry = entries()[0];
        const y = entry.y;
        wait(600);
        verify(entry.y < y);
        overlay.paused = true;
        const stoppedY = entry.y;
        verify(entry.background.visible);
        const shadow = findChild(entry, "commentShadow");
        verify(shadow.item !== null);
        waitForRendering(overlay);
        const shaded = grabImage(overlay);
        overlay.shadowEnabled = false;
        waitForRendering(overlay);
        verify(!shaded.equals(grabImage(overlay)));
        compare(entry.y, stoppedY);
        overlay.textOpacity = 0.5;
        verify(entry.opacity <= 0.5);
        overlay.paused = false;
        tryCompare(overlay, "activeCount", 0, 5000);
        tryCompare(overlay, "visualCount", 0);
    }
    function test_rotated_long_shadow_texture_stays_bounded() {
        overlay.displayMode = "pop";
        overlay.fontSize = 72;
        overlay.receive("長い実況".repeat(200), "right", 0xffffff);
        overlay.paused = true;
        const entry = entries()[0];
        verify(entry.width > 16000);
        const texture = findChild(entry, "commentShadowTexture");
        verify(texture !== null);
        verify(texture.sourceRect.width <= Math.hypot(overlay.width, overlay.height) + 16);
        overlay.clearComments();
        tryCompare(overlay, "visualCount", 0);
        compare(findChild(overlay, "commentShadowTexture"), null);
    }
    function test_size_changes_and_plain_text() {
        overlay.receive("<b>plain</b>\nnext", "right", 0xffffff);
        compare(entries()[0].text, "<b>plain</b> next");
        compare(entries()[0].textFormat, Text.PlainText);
        overlay.height = 3000;
        verify(overlay.laneCount > 64);
        overlay.height = 20;
        compare(overlay.laneCount, 0);
        verify(!overlay.receive("no room", "right", 0xffffff));
        verify(!overlay.receive("invalid", "unknown", 0xffffff));
    }
}
