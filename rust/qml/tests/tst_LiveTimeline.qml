import QtQuick
import QtQuick.Controls
import QtTest
import ".."

Item {
    ApplicationWindow {
        id: host
        width: 860; height: 400; visible: true
        QtObject {
            id: backend
            property string live_timeline: "null"
            property bool seekable: true
            property string transport_error: ""
            property var requests: []
            property int liveRequests: 0
            property var clockEpochs: []
            function seek_timeline(session, value) { requests.push({session: session, value: value}); return true; }
            function return_to_live() { liveRequests++; }
            function timeline_preview(session, value) {
                const data = JSON.parse(live_timeline);
                if (data.session !== session) return "null";
                const program = data.programs.find(p => value >= p.start && value < p.end);
                const clock = clockEpochs.find(c => value >= c.start && value < c.end);
                return JSON.stringify({position: value, utc: clock ? clock.utc + value - clock.start : null, title: program ? program.title : null,
                    available: data.available.some(span => value >= span.start && value < span.end)});
            }
        }
        LiveTimeline { id: timeline; x: 20; y: 100; width: 820; backend: backend }
        TestCase {
            name: "LiveTimeline"
            when: windowShown
            readonly property real minuteMs: 60 * 1000
            function sample() {
                return {session: "one", revision: 1, kind: "live", state: "playing",
                    axis: {start: 0, end: 90 * minuteMs, clock: "elapsed", startUtc: null, endUtc: null},
                    available: [{start: 10 * minuteMs, end: 75 * minuteMs}],
                    live: {position: 75 * minuteMs, utc: null, program: {id: "C", title: "Program C", span: {start: 60 * minuteMs, end: 90 * minuteMs}, progress: 0.5}},
                    viewing: {position: 20 * minuteMs, utc: null, availability: "available", offscreen: false,
                        program: {id: "A", title: "Program A", elapsed: 20 * minuteMs, duration: 30 * minuteMs}},
                    seekTarget: null, boundaries: [30 * minuteMs, 60 * minuteMs],
                    programs: [{id: "A", title: "Program A", start: 0, end: 30 * minuteMs},
                        {id: "B", title: "Program B", start: 30 * minuteMs, end: 60 * minuteMs},
                        {id: "C", title: "Program C", start: 60 * minuteMs, end: 90 * minuteMs}]};
            }
            function publish(data) { backend.live_timeline = JSON.stringify(data); }
            function init() {
                failOnWarning(/.*/);
                backend.seekable = true; backend.transport_error = ""; backend.requests = []; backend.liveRequests = 0;
                backend.clockEpochs = [];
                timeline.width = 820;
                timeline.closing = false;
                timeline.LayoutMirroring.enabled = false; timeline.LayoutMirroring.childrenInherit = true;
                publish(sample());
                host.requestActivate(); tryCompare(host, "active", true);
                waitForRendering(timeline);
                mouseMove(host.contentItem, 5, 5);
            }
            function test_broadcast_progress_cursor_and_history_are_independent_and_centered() {
                const slider = findChild(timeline, "liveSeekSlider");
                const progress = findChild(timeline, "programProgressFill");
                const retained = findChild(timeline, "retainedRanges").itemAt(0);
                const playhead = findChild(timeline, "livePlayhead");
                const markers = findChild(timeline, "programBoundaryMarkers");
                compare(markers.count, 2);
                const track = findChild(timeline, "programTrack");
                compare(slider.from, 0); compare(slider.to, 90 * minuteMs);
                const titles = findChild(timeline, "programSegments");
                verify(titles.itemAt(0).text.includes("Watching · Program A"));
                verify(titles.itemAt(2).text.includes("On air · Program C"));
                for (const hover of [false, true]) {
                    mouseMove(hover ? slider : host.contentItem, hover ? slider.width / 2 : 5, hover ? slider.height / 2 : 5);
                    tryCompare(timeline, "hovered", hover);
                    for (const mirrored of [false, true]) {
                        timeline.LayoutMirroring.enabled = mirrored;
                        compare(progress.y + progress.height / 2, retained.y + retained.height / 2);
                        compare(markers.itemAt(0).y + markers.itemAt(0).height / 2, track.y + track.height / 2);
                        compare(progress.y + progress.height / 2, track.y + track.height / 2);
                        fuzzyCompare(progress.width / track.width, 15 / 90, 0.001);
                        fuzzyCompare(progress.x / track.width, mirrored ? 15 / 90 : 60 / 90, 0.001);
                        fuzzyCompare(retained.width / track.width, 65 / 90, 0.001);
                        fuzzyCompare((playhead.x + playhead.width / 2) / track.width, mirrored ? 70 / 90 : 20 / 90, 0.001);
                    }
                }
            }
            function test_hover_names_old_program_and_rejects_unreceived_portions() {
                const slider = findChild(timeline, "liveSeekSlider");
                const preview = findChild(timeline, "liveSeekPreview");
                mouseMove(slider, slider.width / 2, slider.height / 2);
                tryCompare(preview, "visible", true);
                verify(preview.text.includes("45:00")); verify(preview.text.includes("Program B"));
                compare(backend.requests.length, 0);
                mouseClick(slider, slider.width - 1, slider.height / 2);
                mouseClick(slider, 1, slider.height / 2);
                compare(backend.requests.length, 0);
                mouseClick(slider, slider.width / 2, slider.height / 2);
                compare(backend.requests.length, 1);
                compare(backend.requests[0].session, "one");
                mouseMove(host.contentItem, 5, 5);
                compare(findChild(timeline, "returnToLiveButton"), null);
            }
            function test_joining_mid_program_keeps_full_axis_and_only_retained_part_seekable() {
                // Join at 20:36 during a 20:15–20:42 program; LIVE is 20:38.
                const data = sample();
                const programStart = -21 * minuteMs;
                const programEnd = 6 * minuteMs;
                const livePosition = 2 * minuteMs;
                data.axis = {start: programStart, end: programEnd, clock: "broadcast",
                    startUtc: new Date(2026, 8, 17, 20, 15).getTime(),
                    endUtc: new Date(2026, 8, 17, 20, 42).getTime()};
                data.available = [{start: 0, end: livePosition}];
                data.live = {position: livePosition, utc: data.axis.startUtc + 23 * minuteMs,
                    program: {id: "A", title: "Program A", span: {start: programStart, end: programEnd}, progress: 23 / 27}};
                data.viewing = {position: livePosition, utc: data.live.utc, availability: "available", offscreen: false,
                    program: {id: "A", title: "Program A", elapsed: 23 * minuteMs, duration: 27 * minuteMs}};
                data.programs = [{id: "A", title: "Program A", start: programStart, end: programEnd}];
                data.boundaries = [];
                publish(data);
                const slider = findChild(timeline, "liveSeekSlider");
                const track = findChild(timeline, "programTrack");
                const progress = findChild(timeline, "programProgressFill");
                const retained = findChild(timeline, "retainedRanges").itemAt(0);
                const playhead = findChild(timeline, "livePlayhead");
                compare(slider.from, programStart); compare(slider.to, programEnd);
                fuzzyCompare(progress.x / track.width, 0, 0.001);
                fuzzyCompare(progress.width / track.width, 23 / 27, 0.001);
                fuzzyCompare(retained.x / track.width, 21 / 27, 0.001);
                fuzzyCompare(retained.width / track.width, 2 / 27, 0.001);
                fuzzyCompare((playhead.x + playhead.width / 2) / track.width, 23 / 27, 0.001);
                mouseClick(slider, slider.width * 5 / 27, slider.height / 2);
                mouseClick(slider, slider.width * 26 / 27, slider.height / 2);
                compare(backend.requests.length, 0);
                mouseClick(slider, slider.width * 22 / 27, slider.height / 2);
                compare(backend.requests.length, 1);
                verify(backend.requests[0].value >= 0 && backend.requests[0].value < livePosition);
            }
            function test_drag_freezes_axis_and_sends_expired_target_for_backend_revalidation() {
                const slider = findChild(timeline, "liveSeekSlider");
                mousePress(slider, slider.width / 4, slider.height / 2);
                mouseMove(slider, slider.width / 3, slider.height / 2);
                const frozenTarget = slider.value;
                const data = sample();
                data.axis.start = 30 * minuteMs; data.axis.end = 120 * minuteMs;
                data.available[0].start = 40 * minuteMs;
                publish(data);
                compare(slider.from, 0); compare(slider.to, 90 * minuteMs);
                mouseRelease(slider, slider.width / 3, slider.height / 2);
                compare(backend.requests.length, 1);
                compare(backend.requests[0].value, frozenTarget);
                verify(backend.requests[0].value < data.available[0].start);
                compare(slider.from, data.axis.start); compare(slider.to, data.axis.end);
            }
            function test_channel_change_cancels_old_drag() {
                const slider = findChild(timeline, "liveSeekSlider");
                mousePress(slider, slider.width / 4, slider.height / 2);
                mouseMove(slider, slider.width / 2, slider.height / 2);
                const data = sample(); data.session = "two"; publish(data);
                mouseRelease(slider, slider.width / 2, slider.height / 2);
                compare(backend.requests.length, 0);
            }
            function test_stop_clears_program_labels_and_clocks() {
                backend.live_timeline = "null";
                waitForRendering(timeline);
                compare(findChild(timeline, "programSegments").count, 0);
                compare(findChild(timeline, "programBoundaryClocks").count, 0);
                verify(!findChild(timeline, "timelineEndLabel").visible);
                verify(!findChild(timeline, "livePositionLabel").visible);
            }
            function test_expired_offscreen_frame_and_seek_target_are_distinct() {
                const data = sample();
                data.state = "paused"; data.axis.start = 30 * minuteMs;
                data.available[0].start = 35 * minuteMs;
                data.viewing.availability = "expired"; data.viewing.offscreen = true;
                publish(data);
                verify(findChild(timeline, "expiredPlayheadMarker").visible);
                verify(findChild(timeline, "expiredPlayheadMarker").text.includes("20:00"));
                verify(findChild(timeline, "expiredPlayheadMarker").text.includes("Program A"));
                verify(!findChild(timeline, "livePlayhead").visible);
                compare(findChild(timeline, "liveTimelineStatus").text, "Paused outside retained history");
                data.state = "seeking"; data.seekTarget = 40 * minuteMs; publish(data);
                verify(findChild(timeline, "seekTargetMarker").visible);
                verify(findChild(timeline, "expiredPlayheadMarker").visible);
                compare(findChild(timeline, "liveSeekSlider").from, 30 * minuteMs);
            }
            function test_disabled_retention_keeps_white_progress_and_live_marker() {
                const data = sample(); data.available = []; data.viewing.availability = "disabled";
                backend.seekable = false; publish(data);
                verify(!findChild(timeline, "liveSeekSlider").enabled);
                verify(findChild(timeline, "programProgressFill").visible);
                verify(findChild(timeline, "livePositionMarker").visible);
                compare(findChild(timeline, "retainedRanges").count, 0);
                verify(findChild(timeline, "programBoundaryClocks").itemAt(0).visible);
            }
            function test_returning_to_live_moves_title_emphasis_without_changing_program_axis() {
                const titles = findChild(timeline, "programSegments");
                const slider = findChild(timeline, "liveSeekSlider");
                const boundaries = findChild(timeline, "programBoundaryClocks");
                const originalTitleX = titles.itemAt(2).x;
                const originalClockX = boundaries.itemAt(2).x;
                compare(titles.count, 3);
                compare(titles.itemAt(0).font.weight, Font.DemiBold);
                compare(titles.itemAt(1).text, "Program B");
                compare(titles.itemAt(2).font.weight, Font.Normal);
                const data = sample();
                data.viewing.position = data.live.position;
                data.viewing.program = {id: "C", title: "Program C", elapsed: 15 * minuteMs, duration: 30 * minuteMs};
                publish(data);
                compare(titles.itemAt(0).text, "Program A");
                compare(titles.itemAt(0).font.weight, Font.Normal);
                compare(titles.itemAt(2).text, "Watching · Program C");
                compare(titles.itemAt(2).font.weight, Font.DemiBold);
                compare(titles.itemAt(2).x, originalTitleX);
                compare(boundaries.itemAt(2).x, originalClockX);
                compare(slider.from, data.axis.start); compare(slider.to, data.axis.end);
            }
            function test_boundary_clocks_follow_broadcast_clock_epochs() {
                const data = sample();
                const startUtc = new Date(2026, 8, 19, 20, 0).getTime();
                const correction = 5 * minuteMs;
                backend.clockEpochs = [{start: 0, end: 30 * minuteMs, utc: startUtc},
                    {start: 30 * minuteMs, end: 90 * minuteMs, utc: startUtc + 30 * minuteMs + correction}];
                data.axis.clock = "broadcast";
                data.axis.startUtc = startUtc;
                data.axis.endUtc = startUtc + 90 * minuteMs + correction;
                data.live.utc = startUtc + 75 * minuteMs + correction;
                publish(data);
                const clocks = findChild(timeline, "programBoundaryClocks");
                compare(clocks.count, 3);
                compare(clocks.itemAt(0).text, "20:00");
                compare(clocks.itemAt(1).text, "20:35");
                compare(clocks.itemAt(2).text, "21:05");
                compare(findChild(timeline, "timelineEndLabel").text, "21:35");
                compare(findChild(timeline, "livePositionLabel").text, "LIVE 21:20");
            }
            function test_clock_labels_do_not_overlap_live_or_each_other_at_narrow_widths() {
                const live = findChild(timeline, "livePositionLabel");
                const end = findChild(timeline, "timelineEndLabel");
                const clocks = findChild(timeline, "programBoundaryClocks");
                for (const width of [1392, 852, 532]) {
                    timeline.width = width;
                    for (const mirrored of [false, true]) {
                        timeline.LayoutMirroring.enabled = mirrored;
                        for (const position of [0, 30, 60, 75, 90]) {
                            const data = sample(); data.live.position = position * minuteMs;
                            data.live.utc = new Date(2026, 8, 19, 20, 0).getTime() + data.live.position;
                            publish(data); waitForRendering(timeline);
                            const labels = [live, end];
                            for (let i = 0; i < clocks.count; i++) labels.push(clocks.itemAt(i));
                            const visible = labels.filter(label => label.visible).sort((a, b) => a.x - b.x);
                            verify(live.visible);
                            for (let i = 0; i < visible.length; i++) {
                                verify(visible[i].x >= 0 && visible[i].x + visible[i].width <= timeline.width);
                                if (i > 0) verify(visible[i - 1].x + visible[i - 1].width < visible[i].x);
                            }
                        }
                    }
                }
            }
            function test_gap_is_unfilled_and_unknown_metadata_is_still_seekable() {
                const data = sample();
                data.available = [{start: 10 * minuteMs, end: 30 * minuteMs}, {start: 60 * minuteMs, end: 75 * minuteMs}];
                data.programs = []; data.live.program = null; data.viewing.program = null;
                publish(data);
                compare(findChild(timeline, "retainedRanges").count, 2);
                verify(findChild(timeline, "unknownProgramLabel").visible);
                const slider = findChild(timeline, "liveSeekSlider");
                mouseClick(slider, slider.width / 2, slider.height / 2);
                compare(backend.requests.length, 0);
                mouseClick(slider, slider.width / 4, slider.height / 2);
                compare(backend.requests.length, 1);
            }
            function test_keyboard_uses_the_same_guarded_request() {
                const slider = findChild(timeline, "liveSeekSlider");
                slider.forceActiveFocus(); keyClick(Qt.Key_Right);
                compare(backend.requests.length, 1); compare(backend.requests[0].session, "one");
                timeline.closing = true;
                keyClick(Qt.Key_Right); compare(backend.requests.length, 1);
            }
        }
    }
}
