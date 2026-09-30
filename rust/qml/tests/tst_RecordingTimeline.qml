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
            property string seek_preview_image: ""
            property var previewRequests: []
            function request_seek_preview(milliseconds) { previewRequests.push(milliseconds); }
            function clear_seek_preview() { seek_preview_image = ""; }
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
            property real seek_target_ms: -1
            property real duration_ms: 60000
            property string transport_error: ""
            property bool paused: false
            property bool seeking: false
            property bool ended: false
            property var requests: []
            property bool deferSeekCompletion: false
            function seek_to(value) {
                requests.push(value);
                if (deferSeekCompletion) { seek_target_ms = value; seeking = true; }
                else position_ms = value;
                return true;
            }
            function skip(delta) {
                seek_target_ms = Math.max(window_start_ms, Math.min(window_end_ms - 1,
                    (seek_target_ms >= 0 ? seek_target_ms : position_ms) + delta));
                requests.push(seek_target_ms); seeking = true; return true;
            }
        }
        RecordingTimeline { id: timeline; x: 20; y: 100; width: 680; backend: backend }
        TestInputMethod { id: inputEvents }
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
                backend.seek_target_ms = -1; backend.seeking = false;
                backend.deferSeekCompletion = false;
                backend.previewRequests = [];
                host.contentItem.forceActiveFocus();
                timeline.visible = false; timeline.visible = true;
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
            function test_unavailable_seek_range_keeps_known_duration_and_position() {
                const slider = findChild(timeline, "recordingSeekSlider");
                const label = findChild(timeline, "recordingTime");
                slider.forceActiveFocus();
                backend.seekable = false; backend.window_end_ms = -1;
                compare(slider.enabled, false);
                compare(slider.to, 60000);
                compare(slider.value, 5000);
                compare(label.text, "0:05 / 1:00");
                keyClick(Qt.Key_Right);
                compare(backend.requests.length, 0);
                backend.window_end_ms = 60000; backend.seekable = true;
                compare(slider.enabled, true);
                compare(slider.value, 5000);
                // Stopping clears the session's retained values normally.
                backend.seekable = false; backend.position_ms = -1;
                backend.duration_ms = -1; backend.window_end_ms = -1;
                compare(slider.value, 0);
                compare(label.text, "--:-- / --:--");
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
            function test_image_preview_does_not_seek_and_clears_on_close() {
                const slider = findChild(timeline, "recordingSeekSlider");
                const preview = findChild(timeline, "recordingSeekPreview");
                mouseMove(slider, slider.width / 2, slider.height / 2);
                tryVerify(() => backend.previewRequests.length > 0);
                backend.seek_preview_image = Qt.resolvedUrl("../../../assets/icons/radio.svg");
                const image = findChild(preview, "seekPreviewImage");
                tryCompare(image, "visible", true);
                tryCompare(image, "status", Image.Ready);
                compare(backend.position_ms, 5000);
                compare(backend.requests.length, 0);
                verify(preview.x >= 0 && preview.x + preview.width <= slider.width);
                timeline.closing = true;
                tryCompare(preview, "visible", false);
                compare(backend.seek_preview_image, "");
            }
            function test_keyboard_unknown_duration_and_cancel_on_stop() {
                const slider = findChild(timeline, "recordingSeekSlider");
                slider.forceActiveFocus();
                keyClick(Qt.Key_Right);
                compare(backend.requests.length, 1);
                compare(backend.requests[0], 15000);
                backend.timeshift = false; backend.window_start_ms = 0; backend.liveRequests = 0;
                backend.requests = [];
                mousePress(slider, slider.width * 0.5, slider.height / 2);
                backend.seekable = false;
                mouseRelease(slider, slider.width * 0.5, slider.height / 2);
                compare(backend.requests.length, 0);
                backend.seek_target_ms = -1; backend.seeking = false;
                backend.duration_ms = -1;
                backend.seekable = true;
                compare(slider.enabled, false);
                compare(timeline.timeLabel(-1), "--:--");
                compare(timeline.timeLabel(3671000), "1:01:11");
                backend.duration_ms = 60000;
                compare(slider.value, backend.position_ms);
            }
            function test_repeated_keys_keep_pending_target_and_reverse_direction() {
                const slider = findChild(timeline, "recordingSeekSlider");
                const marker = findChild(timeline, "recordingSeekTargetMarker");
                slider.forceActiveFocus(Qt.TabFocusReason);
                const label = findChild(slider, "seekPositionLabel");
                const ring = findChild(slider, "seekFocusRing");
                keyPress(Qt.Key_Right);
                compare(backend.requests, [15000]); // Accepted on press, before release.
                verify(label.visible); compare(label.text, "0:15");
                tryCompare(ring, "opacity", 1);
                keyRelease(Qt.Key_Right);
                compare(backend.requests.length, 1);
                verify(marker.visible);
                compare(slider.value, 5000);
                compare(findChild(timeline, "recordingTime").text, "0:15 / 1:00");
                keyClick(Qt.Key_Right); keyClick(Qt.Key_Left);
                compare(backend.requests, [15000, 25000, 15000]);
                compare(backend.position_ms, 5000);
                backend.position_ms = 7000; // An old playback sample must not reset intent.
                compare(slider.value, 7000);
                keyClick(Qt.Key_Right);
                compare(backend.seek_target_ms, 25000);
                backend.position_ms = backend.seek_target_ms;
                backend.seek_target_ms = -1; backend.seeking = false;
                verify(!marker.visible);
                compare(slider.value, 25000);
                keyClick(Qt.Key_Right);
                compare(backend.seek_target_ms, 35000);
                verify(inputEvents.forward_key(Qt.Key_Right, Qt.NoModifier, "", true));
                verify(inputEvents.forward_key(Qt.Key_Right, Qt.NoModifier, "", true));
                compare(backend.seek_target_ms, 55000);
                compare(label.text, "0:55");
                // The transient timeout must not erase a retained keyboard focus.
                tryCompare(slider, "feedbackActive", false);
                verify(slider.emphasized); verify(label.visible); compare(ring.opacity, 1);
            }
            function test_transient_feedback_restarts_without_stealing_focus_and_clears_on_disable() {
                const slider = findChild(timeline, "recordingSeekSlider");
                const label = findChild(slider, "seekPositionLabel");
                const ring = findChild(slider, "seekFocusRing");
                const handleX = slider.handle.x;
                backend.skip(10000); slider.flashSeek();
                verify(host.contentItem.activeFocus);
                verify(label.visible); compare(label.text, "0:15"); tryCompare(ring, "opacity", 0);
                tryCompare(findChild(slider, "seekThumb"), "width", Theme.seekThumbActiveSize);
                compare(slider.handle.x, handleX);
                wait(Theme.seekFeedbackDuration * 0.6);
                backend.skip(10000); slider.flashSeek();
                wait(Theme.seekFeedbackDuration * 0.6);
                verify(slider.feedbackActive); compare(label.text, "0:25");
                tryCompare(slider, "feedbackActive", false);
                verify(!label.visible); verify(!slider.emphasized);
                slider.flashSeek();
                backend.seekable = false;
                verify(!slider.feedbackActive); verify(!label.visible);
                backend.seekable = true;
                verify(!slider.feedbackActive);
                slider.flashSeek(); timeline.visible = false; timeline.visible = true;
                verify(!slider.feedbackActive);
                compare(backend.previewRequests.length, 0);
            }
            function test_feedback_label_clamps_at_both_ends_and_overrides_pointer_preview() {
                const slider = findChild(timeline, "recordingSeekSlider");
                const label = findChild(slider, "seekPositionLabel");
                const preview = findChild(timeline, "recordingSeekPreview");
                mouseMove(slider, slider.width / 2, slider.height / 2);
                tryCompare(preview, "visible", true);
                for (const width of [680, 300]) {
                    timeline.width = width;
                    for (const mirrored of [false, true]) {
                        timeline.LayoutMirroring.enabled = mirrored;
                        timeline.LayoutMirroring.childrenInherit = true;
                        for (const target of [0, 59999]) {
                            backend.seek_target_ms = target; slider.flashSeek();
                            verify(label.visible); verify(!preview.visible);
                            verify(label.x >= 0 && label.x + label.width <= slider.width);
                            compare(label.text, target === 0 ? "0:00" : "0:59");
                        }
                    }
                }
                timeline.width = 680;
            }
            function test_drag_keeps_confirmed_position_and_separate_target_until_arrival() {
                const slider = findChild(timeline, "recordingSeekSlider");
                const label = findChild(timeline, "recordingTime");
                backend.deferSeekCompletion = true;
                mousePress(slider, slider.width * 0.25, slider.height / 2);
                mouseMove(slider, slider.width * 0.6, slider.height / 2);
                const target = slider.value;
                const time = label.text;
                mouseRelease(slider, slider.width * 0.6, slider.height / 2);
                waitForRendering(timeline);
                compare(slider.value, 5000);
                compare(label.text, time);
                compare(backend.position_ms, 5000);
                verify(findChild(timeline, "recordingSeekTargetMarker").visible);
                backend.position_ms = target;
                backend.seek_target_ms = -1; backend.seeking = false;
                compare(slider.value, target);
                compare(label.text, time);
                backend.position_ms = target + 1000;
                compare(slider.value, target + 1000);
                keyClick(Qt.Key_Right);
                compare(slider.value, backend.position_ms);
                verify(backend.seek_target_ms > backend.position_ms);
                backend.seek_target_ms = -1; backend.seeking = false;
                compare(slider.value, backend.position_ms);
                compare(label.text, timeline.timeLabel(backend.position_ms) + " / 1:00");
            }
            function test_keyboard_mirroring_and_disabled_input() {
                const slider = findChild(timeline, "recordingSeekSlider");
                timeline.LayoutMirroring.enabled = true;
                timeline.LayoutMirroring.childrenInherit = true;
                slider.forceActiveFocus();
                keyClick(Qt.Key_Left);
                compare(backend.seek_target_ms, 15000);
                keyClick(Qt.Key_Right);
                compare(backend.seek_target_ms, 5000);
                timeline.closing = true;
                keyClick(Qt.Key_Left);
                compare(backend.requests.length, 2);
            }
        }
    }
}
