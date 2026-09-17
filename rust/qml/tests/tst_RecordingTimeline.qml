import QtQuick
import QtQuick.Controls
import QtTest
import ".."
Item {
    ApplicationWindow {
        id: host
        width: 720; height: 320; visible: true
        QtObject {
            id: backend
            property bool recording: true
            property string current_program_data: "null"
            property real program_progress: 0
            property bool timeshift: false
            property string timeshift_program_boundaries: "[]"
            property real window_start_ms: 0
            property real window_end_ms: duration_ms
            property real live_delay_ms: duration_ms - position_ms
            property int liveRequests: 0
            function return_to_live() { liveRequests++; }
            property bool seekable: true
            property real position_ms: 5000
            property real duration_ms: 60000
            property string transport_error: ""
            property bool paused: false
            property bool seeking: false
            property bool ended: false
            property var requests: []
            function seek_to(value) { requests.push(value); position_ms = value; return true; }
        }
        RecordingTimeline { id: timeline; x: 20; y: 100; width: 680; backend: backend }
        TestCase {
            name: "RecordingTimeline"
            when: windowShown
            function init() {
                failOnWarning(/.*/);
                backend.seekable = true; backend.position_ms = 5000; backend.duration_ms = 60000;
                backend.timeshift = false; backend.window_start_ms = 0; backend.liveRequests = 0;
                backend.timeshift_program_boundaries = "[]";
                backend.window_end_ms = Qt.binding(function() { return backend.duration_ms; });
                backend.recording = true; backend.current_program_data = "null"; backend.program_progress = 0;
                backend.requests = []; timeline.closing = false;
                timeline.LayoutMirroring.enabled = false;
                host.requestActivate();
                tryCompare(host, "active", true);
                waitForRendering(timeline);
                mouseMove(host.contentItem, 5, 5);
            }
            function test_live_window_hover_uses_retained_start_and_returns_to_live() {
                backend.recording = false; backend.timeshift = true;
                backend.window_start_ms = 70000; backend.duration_ms = 100000; backend.position_ms = 73000;
                const slider = findChild(timeline, "recordingSeekSlider");
                compare(slider.from, 70000); compare(slider.to, 100000);
                mouseMove(slider, slider.width / 2, slider.height / 2);
                const preview = findChild(timeline, "recordingSeekPreview");
                tryCompare(preview, "visible", true); compare(preview.text, "1:25");
                compare(backend.requests.length, 0);
                const live = findChild(timeline, "returnToLiveButton");
                verify(live.visible); waitForRendering(live);
                mouseMove(host.contentItem, 5, 5); tryCompare(preview, "visible", false);
                mouseClick(live);
                compare(backend.liveRequests, 1);
            }
            function test_program_and_retention_share_one_axis_and_reject_future_seeks() {
                backend.recording = false; backend.timeshift = true;
                const minuteMs = 60 * 1000;
                backend.window_start_ms = 2 * minuteMs;
                backend.window_end_ms = 10 * minuteMs;
                backend.position_ms = 8 * minuteMs;
                backend.program_progress = 0.4;
                backend.current_program_data = JSON.stringify({playbackStartMs: 0, playbackEndMs: 20 * minuteMs, startAt: 0});
                const slider = findChild(timeline, "recordingSeekSlider");
                const retained = findChild(timeline, "retainedRangeFill");
                const progress = findChild(timeline, "programProgressFill");
                waitForRendering(timeline);
                compare(slider.from, 0); compare(slider.to, 20 * minuteMs);
                verify(retained.visible); compare(retained.color, "#9caf9f"); compare(progress.color, "#f4f5f3");
                fuzzyCompare(retained.x / retained.parent.width, 0.1, 0.001);
                fuzzyCompare(retained.width / retained.parent.width, 0.4, 0.001);
                fuzzyCompare(progress.width / progress.parent.width, 0.4, 0.001);
                mouseClick(slider, slider.width * 0.8, slider.height / 2);
                compare(backend.requests.length, 0);
                mouseClick(slider, 1, slider.height / 2);
                compare(backend.requests.length, 0);
                mouseClick(slider, slider.width * 0.3, slider.height / 2);
                compare(backend.requests.length, 1);
                verify(backend.requests[0] >= backend.window_start_ms && backend.requests[0] <= backend.window_end_ms);
                // Retained data from the preceding show stays reachable.
                backend.current_program_data = JSON.stringify({playbackStartMs: 5 * minuteMs, playbackEndMs: 20 * minuteMs, startAt: 0});
                compare(slider.from, 2 * minuteMs);
                timeline.LayoutMirroring.enabled = true; timeline.LayoutMirroring.childrenInherit = true;
                fuzzyCompare(retained.x / retained.parent.width, 1 - timeline.fraction(backend.window_end_ms), 0.001);
            }
            function test_live_without_retention_keeps_program_progress() {
                backend.recording = false; backend.timeshift = false; backend.seekable = false;
                backend.program_progress = 0.6;
                backend.current_program_data = JSON.stringify({playbackStartMs: -4000, playbackEndMs: 6000});
                const slider = findChild(timeline, "recordingSeekSlider");
                const progress = findChild(timeline, "programProgressFill");
                waitForRendering(timeline);
                verify(!slider.enabled); verify(!findChild(timeline, "retainedRangeFill").visible);
                fuzzyCompare(progress.width / progress.parent.width, 0.6, 0.001);
                backend.current_program_data = JSON.stringify({name: "No broadcast clock"});
                fuzzyCompare(progress.width / progress.parent.width, 0.6, 0.001);
            }
            function test_program_ticks_follow_retention_and_mirroring_with_exact_shared_center() {
                const secondMs = 1000;
                backend.recording = false; backend.timeshift = true;
                backend.window_start_ms = 5 * secondMs; backend.window_end_ms = 55 * secondMs;
                backend.current_program_data = JSON.stringify({playbackStartMs: 0, playbackEndMs: 60 * secondMs});
                backend.timeshift_program_boundaries = JSON.stringify([0, 20, 40, 60].map(s => s * secondMs));
                const markers = findChild(timeline, "programBoundaryMarkers");
                const retained = findChild(timeline, "retainedRangeFill");
                const progress = findChild(timeline, "programProgressFill");
                const track = findChild(timeline, "programTrack");
                const slider = findChild(timeline, "recordingSeekSlider");
                tryCompare(markers, "count", 2);
                for (const hovering of [false, true]) {
                    mouseMove(hovering ? slider : host.contentItem, hovering ? slider.width / 2 : 5, hovering ? slider.height / 2 : 5);
                    tryCompare(timeline, "hovered", hovering);
                    for (const mirrored of [false, true]) {
                        timeline.LayoutMirroring.enabled = mirrored; timeline.LayoutMirroring.childrenInherit = true;
                        compare(retained.y + retained.height / 2, track.y + track.height / 2);
                        compare(progress.y + progress.height / 2, track.y + track.height / 2);
                        const tick = markers.itemAt(0);
                        compare(tick.y + tick.height / 2, track.y + track.height / 2);
                        fuzzyCompare((tick.x + tick.width / 2) / track.width, mirrored ? 2 / 3 : 1 / 3, 0.001);
                    }
                }
                backend.window_start_ms = 25 * secondMs;
                tryCompare(markers, "count", 1);
                backend.timeshift = false;
                tryCompare(markers, "count", 0);
            }
            function test_partial_index_remains_seekable_before_total_duration_is_known() {
                backend.duration_ms = -1;
                backend.window_end_ms = 20000;
                const slider = findChild(timeline, "recordingSeekSlider");
                verify(slider.enabled); compare(slider.to, 20000);
                mouseMove(slider, slider.width / 2, slider.height / 2);
                const preview = findChild(timeline, "recordingSeekPreview");
                tryCompare(preview, "visible", true); compare(preview.text, "0:10");
                verify(findChild(timeline, "recordingTime").text.endsWith(" / --:--"));
                mouseClick(slider, slider.width / 2, slider.height / 2);
                compare(backend.requests.length, 1);
                compare(backend.requests[0], 10000);
            }
            function test_hover_previews_position_without_seeking() {
                const slider = findChild(timeline, "recordingSeekSlider");
                const preview = findChild(timeline, "recordingSeekPreview");
                for (const sample of [[1, "0:00"], [slider.width / 2, "0:30"], [slider.width - 1, "1:00"]]) {
                    mouseMove(slider, sample[0], slider.height / 2);
                    tryCompare(preview, "visible", true);
                    compare(preview.text, sample[1]);
                    verify(preview.x >= 0 && preview.x + preview.width <= slider.width);
                    compare(backend.position_ms, 5000);
                    compare(backend.requests.length, 0);
                }
                mouseMove(host.contentItem, 5, 5);
                tryCompare(preview, "visible", false);
            }
            function test_hover_tracks_duration_and_mirroring_and_hides_when_disabled() {
                const slider = findChild(timeline, "recordingSeekSlider");
                const preview = findChild(timeline, "recordingSeekPreview");
                backend.duration_ms = 7200000;
                mouseMove(slider, slider.width / 2, slider.height / 2);
                tryCompare(preview, "visible", true);
                compare(preview.text, "1:00:00");
                timeline.LayoutMirroring.enabled = true;
                timeline.LayoutMirroring.childrenInherit = true;
                mouseMove(slider, 1, slider.height / 2);
                compare(preview.text, "2:00:00");
                backend.duration_ms = 912741;
                compare(preview.text, "15:12");
                timeline.closing = true;
                tryCompare(preview, "visible", false);
                timeline.closing = false;
                backend.duration_ms = -1;
                compare(preview.visible, false);
                compare(backend.requests.length, 0);
            }
            function test_drag_commits_once_and_preserves_preview_during_position_updates() {
                const slider = findChild(timeline, "recordingSeekSlider");
                mousePress(slider, slider.width * 0.25, slider.height / 2);
                mouseMove(slider, slider.width * 0.6, slider.height / 2);
                verify(timeline.pressed);
                compare(backend.requests.length, 0);
                const preview = slider.value;
                backend.position_ms = 9000;
                compare(slider.value, preview);
                mouseRelease(slider, slider.width * 0.6, slider.height / 2);
                compare(backend.requests.length, 1);
                verify(Math.abs(backend.requests[0] - 36000) < 2000);
                verify(!timeline.pressed);
            }
            function test_keyboard_unknown_duration_and_cancel_on_stop() {
                const slider = findChild(timeline, "recordingSeekSlider");
                slider.forceActiveFocus();
                keyClick(Qt.Key_Right);
                compare(backend.requests.length, 1);
                compare(backend.requests[0], 6000);
                backend.timeshift = false; backend.window_start_ms = 0; backend.liveRequests = 0;
                backend.requests = [];
                mousePress(slider, slider.width * 0.5, slider.height / 2);
                backend.seekable = false;
                mouseRelease(slider, slider.width * 0.5, slider.height / 2);
                compare(backend.requests.length, 0);
                backend.duration_ms = -1;
                backend.seekable = true;
                compare(slider.enabled, false);
                compare(timeline.timeLabel(-1), "--:--");
                compare(timeline.timeLabel(3671000), "1:01:11");
                backend.duration_ms = 60000;
                compare(slider.value, backend.position_ms);
            }
        }
    }
}
