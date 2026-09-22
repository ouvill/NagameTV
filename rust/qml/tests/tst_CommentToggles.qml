import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer 1.0
import ".."

Item {
    ApplicationWindow {
        id: host
        visible: true
        width: 1000
        height: 560
        TestCase {
            id: testCase
            name: "CommentToggles"
            when: windowShown
            width: host.width
            height: host.height
            visible: true
            QtObject {
                id: backend
                property bool enabled: true
                property bool danmaku: false
                property bool stats: false
                property bool shadow: true
                property real size: 21
                property int requests: 0
                function setDanmaku(value) { danmaku = value; requests++; }
            }
            SettingsToggle {
                id: setting
                width: 480
                text: "Show comments over the video"
                description: "Settings row"
                enabled: backend.enabled
                checked: backend.danmaku
                onToggled: backend.setDanmaku(checked)
            }
            ProgramSidebar {
                channelModel: ChannelFixture {}
                id: sidebar
                x: 600
                width: 400
                height: host.height
                targetWindow: host
                programJson: "null"
                iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
                page: ProgramSidebar.Playback
                commentsEnabled: backend.enabled
                danmakuEnabled: backend.danmaku
                textSize: backend.size
                statsVisible: backend.stats
                shadowEnabled: backend.shadow
                onDanmakuRequested: function(value) { backend.setDanmaku(value); }
                onAdjusted: function(size, opacity, speed) { backend.size = size; }
                onShadowRequested: function(value) { backend.shadow = value; }
                onStatsRequested: function(value) { backend.stats = value; }
                onPageRequested: function(next) { page = next; }
            }
            SignalSpy { id: statsRequests; target: sidebar; signalName: "statsRequested" }
            SignalSpy { id: timeshiftRequests; target: sidebar; signalName: "timeshiftSettingsRequested" }
            function reveal(control) {
                const scroll = findChild(sidebar, "sidebarPlaybackSettings");
                const flickable = scroll.contentItem;
                const point = control.mapToItem(flickable.contentItem, 0, 0);
                flickable.contentY = Math.max(0, Math.min(point.y - 10, flickable.contentHeight - flickable.height));
                waitForRendering(scroll);
            }
            function sidebarToggle() { return findChild(sidebar, "playbackDanmakuToggle"); }
            function verifyState(value, requests) {
                compare(backend.danmaku, value);
                compare(backend.requests, requests);
                compare(setting.checked, value);
                compare(sidebarToggle().checked, value);
            }
            function init() {
                failOnWarning(/.*/);
                backend.enabled = true; backend.danmaku = false;
                backend.stats = false; backend.shadow = true; backend.size = 21;
                backend.requests = 0;
                sidebar.page = ProgramSidebar.Playback;
                statsRequests.clear(); timeshiftRequests.clear();
                host.requestActivate(); tryCompare(host, "active", true);
                reveal(sidebarToggle());
            }
            function test_surfaces_stay_in_sync_after_mouse_keyboard_and_external_changes() {
                mouseClick(setting, 20, setting.height / 2);
                verifyState(true, 1);
                const toggle = sidebarToggle();
                mousePress(toggle, 0, toggle.height / 2);
                wait(120);
                mouseRelease(toggle, 0, toggle.height / 2);
                verifyState(false, 2);
                toggle.forceActiveFocus(); keyClick(Qt.Key_Space);
                verifyState(true, 3);
                backend.danmaku = false; verifyState(false, 3);
                keyClick(Qt.Key_Space); verifyState(true, 4);
            }
            function test_disabled_controls_do_not_request_changes() {
                backend.enabled = false;
                mouseClick(setting); mouseClick(sidebarToggle());
                verifyState(false, 0);
            }
            function test_stats_and_timeshift_remain_reachable_in_short_sidebar() {
                const stats = findChild(sidebar, "playbackStatsToggle");
                reveal(stats); mouseClick(stats);
                compare(backend.stats, true); compare(statsRequests.count, 1);
                stats.forceActiveFocus(); keyClick(Qt.Key_Space);
                compare(backend.stats, false); compare(statsRequests.count, 2);
                const timeshift = findChild(sidebar, "playbackTimeshiftSettings");
                reveal(timeshift); mouseClick(timeshift);
                compare(timeshiftRequests.count, 1);
                compare(sidebar.page, ProgramSidebar.Playback);
            }
            function test_shadow_and_large_font_updates_survive_tab_changes() {
                const shadow = findChild(sidebar, "playbackShadowToggle");
                reveal(shadow); mouseClick(shadow); compare(backend.shadow, false);
                backend.shadow = true; compare(shadow.checked, true);
                const size = findChild(sidebar, "danmakuTextSize");
                reveal(size); size.forceActiveFocus();
                for (let value = 21; value < 72; ++value) keyClick(Qt.Key_Right);
                compare(backend.size, 72);
                mouseClick(findChild(sidebar, "programSidebarTab"));
                compare(sidebar.page, ProgramSidebar.Program);
                mouseClick(findChild(sidebar, "playbackSidebarTab"));
                compare(sidebar.page, ProgramSidebar.Playback);
                compare(size.value, 72);
                backend.enabled = false; reveal(shadow); mouseClick(shadow);
                compare(backend.shadow, true);
            }
        }
    }
}
