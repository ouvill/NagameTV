import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer
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
                backend.window_end_ms = Qt.binding(function() { return backend.duration_ms; });
                backend.recording = true; backend.current_program_data = "null"; backend.program_progress = 0;
                backend.requests = []; timeline.closing = false;
                timeline.LayoutMirroring.enabled = false;
                host.requestActivate();
                tryCompare(host, "active", true);
                waitForRendering(timeline);
                mouseMove(host.contentItem, 5, 5);
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
