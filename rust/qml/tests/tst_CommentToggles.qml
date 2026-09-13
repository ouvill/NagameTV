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
                property int requests: 0
                function setDanmaku(value) { danmaku = value; requests++; }
            }
            CommentModel { id: comments }
            SettingsToggle {
                id: setting
                width: 480
                text: "Show comments over the video"
                description: "Settings row"
                enabled: backend.enabled
                checked: backend.danmaku
                onToggled: backend.setDanmaku(checked)
            }
            IconAction {
                id: playbackButton
                x: 500
                y: host.height - 64
                iconSource: Qt.resolvedUrl("../../../assets/icons/settings-2.svg")
                tip: "Playback settings"
                active: playback.visible
                onClicked: playback.toggle()
            }
            PlaybackSettings {
                id: playback
                toggleButton: playbackButton
                commentsEnabled: backend.enabled
                danmakuEnabled: backend.danmaku
                textSize: 21
                textOpacity: 1
                speed: 1
                statsVisible: backend.stats
                onDanmakuRequested: function(value) { backend.setDanmaku(value); }
                onStatsRequested: function(value) { backend.stats = value; }
            }
            ProgramSidebar {
                id: sidebar
                x: 600
                width: 400
                height: host.height
                targetWindow: host
                programJson: "null"
                commentModel: comments
                iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
                page: ProgramSidebar.Comments
                commentsEnabled: backend.enabled
                danmakuEnabled: backend.danmaku
                onDanmakuRequested: function(value) { backend.setDanmaku(value); }
            }
            SignalSpy { id: statsRequests; target: playback; signalName: "statsRequested" }
            function playbackToggle() { return findChild(playback.contentItem, "playbackDanmakuToggle"); }
            function sidebarToggle() { return findChild(sidebar, "sidebarDanmakuToggle"); }
            function verifyState(value, requests) {
                compare(backend.danmaku, value);
                compare(backend.requests, requests);
                compare(setting.checked, value);
                compare(playbackToggle().checked, value);
                compare(sidebarToggle().checked, value);
            }
            function init() {
                failOnWarning(/.*/);
                backend.enabled = true;
                backend.danmaku = false;
                backend.stats = false;
                backend.requests = 0;
                statsRequests.clear();
                host.requestActivate();
                tryCompare(host, "active", true);
                compare(setting.enabled, true);
            }
            function cleanup() { playback.close(); }
            function test_all_surfaces_stay_in_sync_after_mouse_keyboard_and_external_changes() {
                // The settings label remains clickable, beyond the small indicator.
                mouseClick(setting, 20, setting.height / 2);
                verifyState(true, 1);
                playback.open();
                tryCompare(playback, "opened", true);
                const compact = playbackToggle();
                // Press feedback must not shrink the logical click target.
                mousePress(compact, 0, compact.height / 2);
                wait(120);
                mouseRelease(compact, 0, compact.height / 2);
                verifyState(false, 2);
                playback.close();
                tryCompare(playback, "visible", false);
                sidebarToggle().forceActiveFocus();
                keyClick(Qt.Key_Space);
                verifyState(true, 3);
                backend.danmaku = false;
                verifyState(false, 3);
                keyClick(Qt.Key_Space);
                verifyState(true, 4);
            }
            function test_disabled_controls_do_not_request_changes() {
                backend.enabled = false;
                mouseClick(setting);
                mouseClick(sidebarToggle());
                playback.open();
                tryCompare(playback, "opened", true);
                mouseClick(playbackToggle());
                verifyState(false, 0);
            }
            function test_playback_stats_mouse_and_keyboard_toggles_keep_menu_open() {
                playback.open();
                tryCompare(playback, "opened", true);
                const stats = findChild(playback.contentItem, "playbackStatsToggle");
                mouseClick(stats);
                compare(backend.stats, true);
                compare(statsRequests.count, 1);
                compare(playback.opened, true);
                stats.forceActiveFocus();
                keyClick(Qt.Key_Space);
                compare(backend.stats, false);
                compare(statsRequests.count, 2);
                compare(playback.opened, true);
            }
            function test_playback_button_closes_without_reopening_and_dismissal_still_works() {
                mouseClick(playbackButton);
                tryCompare(playback, "opened", true);
                mousePress(playbackButton);
                wait(120);
                compare(playback.opened, true);
                mouseRelease(playbackButton);
                tryCompare(playback, "visible", false);
                mouseClick(playbackButton);
                tryCompare(playback, "opened", true);
                keyClick(Qt.Key_Escape);
                tryCompare(playback, "visible", false);
                mouseClick(playbackButton);
                tryCompare(playback, "opened", true);
                mouseClick(testCase, 20, 130);
                tryCompare(playback, "visible", false);
            }
        }
    }
}
