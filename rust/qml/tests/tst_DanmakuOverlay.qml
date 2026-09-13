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
        Viewer.DanmakuOverlay { width: 640; height: 480; fontSize: 21; textOpacity: 1; speed: 2 }
    }
    property var overlay
    function entries() { return Array.from(overlay.visuals.values()); }
    function initTestCase() { failOnWarning(/.*/); }
    function init() {
        overlay = createTemporaryObject(component, this);
        verify(overlay !== null);
    }
    function test_burst_and_clear_reuse_are_driven_by_core() {
        for (let i = 0; i < 1000; ++i)
            overlay.receive("comment " + i, "right", 0xffffff);
        compare(overlay.activeCount, overlay.laneCount);
        compare(entries().length, overlay.activeCount);
        overlay.clearComments();
        compare(overlay.activeCount, 0);
        compare(entries().length, 0);
        overlay.receive("new channel", "right", 0xffffff);
        compare(overlay.activeCount, 1);
        tryVerify(() => overlay.visualCount === 1);
        compare(entries()[0].text, "new channel");
        overlay.clearComments();
        overlay.clearComments();
        tryVerify(() => overlay.visualCount === 0);
    }
    function test_dynamic_rows_and_no_fixed_64_cap() {
        const initial = overlay.laneCount;
        overlay.height = 3000;
        verify(overlay.laneCount > 64);
        for (let i = 0; i < 100; ++i)
            overlay.receive("fixed", "top", 0xffffff);
        compare(overlay.activeCount, Math.min(100, overlay.laneCount));
        compare(entries().length, overlay.activeCount);
        overlay.fontSize = 72;
        compare(overlay.activeCount, 0);
        overlay.height = 480;
        verify(overlay.laneCount < initial);
        overlay.height = 20;
        compare(overlay.laneCount, 0);
        verify(!overlay.receive("no room", "right", 0xffffff));
    }
    function test_typed_render_commands_and_animation() {
        overlay.receive("scroll", "right", 0x123456);
        overlay.receive("top", "top", 0);
        overlay.receive("bottom", "bottom", 0xffffff);
        const visible = entries();
        const scroll = visible[0];
        const top = visible[1];
        const bottom = visible[2];
        compare(scroll.x, overlay.width);
        compare(scroll.duration, 2500);
        compare(scroll.color.toString(), "#123456");
        compare(top.color.toString(), "#000000");
        compare(top.duration, 2000);
        compare(bottom.duration, 2000);
        compare(top.x, (overlay.width - top.width) / 2);
        verify(bottom.mapToItem(overlay, 0, 0).y > top.mapToItem(overlay, 0, 0).y);
        wait(80);
        verify(scroll.x < overlay.width);
        compare(top.x, (overlay.width - top.width) / 2);
        overlay.fullScreen = true;
        overlay.receive("full screen", "right", 0xffffff);
        overlay.receive("fixed", "top", 0xffffff);
        compare(entries()[0].duration, 4000);
        compare(entries()[1].duration, 3000);
    }
    function test_hiding_plain_text_and_invalid_input() {
        overlay.receive("visible", "right", 0xffffff);
        overlay.visible = false;
        verify(!overlay.receive("hidden", "right", 0xffffff));
        compare(overlay.activeCount, 0);
        tryVerify(() => overlay.visualCount === 0);
        overlay.visible = true;
        overlay.receive("<b>plain text</b>\nnext", "right", 0xffffff);
        compare(entries()[0].textFormat, Text.PlainText);
        compare(entries()[0].text, "<b>plain text</b> next");
        verify(!overlay.receive("x", "invalid", 0xffffff));
        verify(!overlay.receive("x", "right", 0x1000000));
    }
    function test_only_own_comment_gets_a_thin_frame_and_keeps_its_lifetime() {
        overlay.receive("same text", "right", 0xffffff);
        overlay.receive("same text", "right", 0xffffff, true);
        const other = entries()[0];
        const own = entries()[1];
        verify(!other.own);
        verify(!other.background.visible);
        verify(own.own);
        verify(own.background.visible);
        compare(own.background.border.width, 1);
        compare(own.background.border.color.toString(), "#ffe066");
        compare(own.width, other.width + 6);
        compare(own.duration, other.duration);
        wait(80);
        verify(own.x < overlay.width);
        compare(own.background.width, own.width);
        overlay.clearComments();
        tryVerify(() => overlay.visualCount === 0);
        overlay.receive("same text", "right", 0xffffff);
        verify(!entries()[0].own);
        verify(!entries()[0].background.visible);
    }
    function test_shadow_changes_pixels_without_replacing_comments_data() {
        return [{tag: "small", size: 14}, {tag: "normal", size: 21}, {tag: "large", size: 72}];
    }
    function test_shadow_changes_pixels_without_replacing_comments(data) {
        overlay.fontSize = data.size;
        verify(overlay.receive("Shadow あいう", "top", 0xffffff, true));
        overlay.paused = true;
        const entry = entries()[0];
        const shadow = findChild(entry, "commentShadow");
        verify(shadow.visible);
        compare(entry.font.pixelSize, data.size);
        waitForRendering(overlay);
        const before = grabImage(overlay);
        const blur = findChild(shadow, "commentShadowBlur");
        verify(blur !== null);
        blur.blur = 0;
        waitForRendering(overlay);
        verify(!before.equals(grabImage(overlay)));
        blur.blur = 1;
        waitForRendering(overlay);
        verify(before.equals(grabImage(overlay)));
        overlay.shadowEnabled = false;
        verify(!shadow.visible);
        compare(shadow.item, null);
        waitForRendering(overlay);
        verify(!before.equals(grabImage(overlay)));
        compare(entries()[0], entry);
        compare(overlay.activeCount, 1);
        verify(entry.background.visible);
        compare(entry.background.border.width, 1);
        overlay.shadowEnabled = true;
        waitForRendering(overlay);
        verify(before.equals(grabImage(overlay)));
        overlay.clearComments();
        tryVerify(() => overlay.visualCount === 0);
    }
    function test_long_comment_shadow_crops_to_video_and_releases_effects() {
        overlay.fontSize = 72;
        verify(overlay.receive("あいうえお".repeat(200), "right", 0xffffff));
        overlay.paused = true;
        const entry = entries()[0];
        verify(entry.width > 16000);
        const shadow = findChild(entry, "commentShadow");
        const texture = findChild(shadow, "commentShadowTexture");
        const blur = findChild(shadow, "commentShadowBlur");
        verify(texture !== null);
        verify(blur !== null);
        for (const x of [-1000, -12000, -18000]) {
            entry.x = x;
            verify(waitForRendering(overlay));
            verify(texture.sourceRect.width <= overlay.width + 16);
            verify(blur.width <= overlay.width + 16);
            // The captured strip covers the viewport as the long text moves.
            verify(blur.mapToItem(overlay, 0, 0).x <= 0);
            verify(blur.mapToItem(overlay, blur.width, 0).x >= overlay.width);
        }
        overlay.clearComments();
        tryVerify(() => overlay.visualCount === 0);
        compare(findChild(overlay, "commentShadowTexture"), null);
        compare(findChild(overlay, "commentShadowBlur"), null);
    }
    function test_pause_resume_and_core_expiration() {
        overlay.receive("test", "right", 0xffffff);
        wait(80);
        overlay.paused = true;
        const entry = entries()[0];
        const stoppedX = entry.x;
        wait(120);
        compare(entry.x, stoppedX);
        compare(overlay.activeCount, 1);
        verify(!overlay.receive("paused", "right", 0xffffff));
        overlay.paused = false;
        tryVerify(() => overlay.activeCount === 0, 4000);
        compare(entries().length, 0);
        tryVerify(() => overlay.visualCount === 0);
    }

    function screenY(entry) { return entry.mapToItem(overlay, 0, 0).y; }
    function test_controls_move_existing_comments_with_horizontal_motion_preserved() {
        overlay.receive("scroll", "right", 0xffffff);
        overlay.receive("fixed", "top", 0xffffff);
        overlay.receive("bottom", "bottom", 0xffffff);
        wait(80);
        const original = entries();
        const scroll = original[0];
        const bottom = original[2];
        const xBefore = scroll.x;
        const yBefore = screenY(scroll);
        const bottomBefore = screenY(bottom);
        overlay.titleBottomInVideo = 130;
        overlay.titleOverlapsVideo = true;
        overlay.controlsTopInVideo = 350;
        overlay.controlsOverlapVideo = true;
        compare(overlay.activeCount, 3);
        compare(entries()[0], scroll);
        wait(60);
        verify(screenY(scroll) > yBefore);
        verify(screenY(scroll) < overlay.controller.flow_origin);
        tryVerify(() => screenY(scroll) >= 146, 1000);
        verify(screenY(bottom) < bottomBefore);
        verify(screenY(bottom) + bottom.height <= 334);
        verify(scroll.x < xBefore);
        overlay.receive("new fixed", "top", 0xffffff);
        verify(screenY(entries()[3]) >= 146);
        for (let i = 0; i < 3; ++i) {
            overlay.titleOverlapsVideo = false;
            overlay.controlsOverlapVideo = false;
            wait(30);
            overlay.titleOverlapsVideo = true;
            overlay.controlsOverlapVideo = true;
            compare(entries()[0], scroll);
            compare(overlay.activeCount, 4);
        }
        overlay.titleOverlapsVideo = false;
        overlay.controlsOverlapVideo = false;
        wait(220);
        compare(screenY(scroll), yBefore);
        compare(screenY(bottom), bottomBefore);
        tryVerify(() => overlay.activeCount === 0, 4000);
        tryVerify(() => overlay.visualCount === 0);
    }
    function test_full_height_and_new_arrivals_during_reversal_stay_inside_video() {
        for (let i = 0; i < overlay.laneCount; ++i)
            overlay.receive("row " + i, "top", 0xffffff);
        const count = overlay.activeCount;
        const original = entries();
        overlay.titleBottomInVideo = 130;
        overlay.titleOverlapsVideo = true;
        overlay.controlsTopInVideo = 300;
        overlay.controlsOverlapVideo = true;
        wait(200);
        compare(overlay.activeCount, count);
        for (let i = 0; i < original.length; ++i) {
            compare(entries()[i], original[i]);
            verify(screenY(original[i]) >= 40);
            verify(screenY(original[i]) + original[i].height <= overlay.height - 24);
            if (i > 0)
                verify(screenY(original[i]) >= screenY(original[i-1]) + original[i-1].height);
        }
        overlay.clearComments();
        overlay.receive("new", "top", 0xffffff);
        overlay.titleOverlapsVideo = false;
        overlay.controlsOverlapVideo = false;
        for (let frame = 0; frame < 12; ++frame) {
            overlay.receive("arriving " + frame, "top", 0xffffff);
            for (const entry of entries()) {
                verify(screenY(entry) >= 40 - 0.01);
                verify(screenY(entry) + entry.height <= overlay.height - 24 + 0.01);
            }
            wait(20);
        }
    }
}
