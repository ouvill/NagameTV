import QtQuick
import QtQuick.Window
import QtTest
import MinimalViewer as Viewer

// Match Main.qml's unbound Loader scope: a helper named video in VideoStats
// must not shadow the enclosing video item used by these property bindings.
TestCase {
    name: "VideoStats"
    when: windowShown
    width: 800
    height: 600
    Component {
        id: fixture
        Item {
            id: video
            width: 640
            height: 360
            property alias stats: loader.item
            Loader {
                id: loader
                sourceComponent: Component {
                    Viewer.VideoStats {
                        width: 510
                        backend: QtObject {
                            function video_stats() { return "{}"; }
                        }
                        closeIcon: Qt.resolvedUrl("../../../assets/icons/x.svg")
                        viewportSize: Qt.size(video.width, video.height)
                        viewportDpr: video.Screen.devicePixelRatio
                    }
                }
            }
        }
    }
    function initTestCase() { failOnWarning(/.*/); }
    function test_receive_latency_availability_and_distribution() {
        const item = createTemporaryObject(fixture, this);
        for (const latency of [undefined, { status: "unavailable" }]) {
            item.stats.snapshot = { receive_latency: latency };
            compare(item.stats.metric("latency"), "—");
            compare(item.stats.metric("latencyDistribution"), "—");
        }
        item.stats.snapshot = { receive_latency: { status: "waiting" } };
        compare(item.stats.metric("latency"), qsTranslate("Main", "Waiting for measurement"));
        item.stats.snapshot = { receive_latency: { status: "measuring", latest_ms: 0, median_ms: 123.4, p95_ms: 234.5, samples: 60 } };
        compare(item.stats.metric("latency"), "0.0 ms");
        compare(item.stats.metric("latencyDistribution"), "123.4 / 234.5 ms · " + qsTranslate("Main", "%1 samples").arg(60));
        item.stats.snapshot = { receive_latency: { status: "waiting" } };
        compare(item.stats.metric("latencyDistribution"), "—");
    }
    function test_short_panel_can_scroll_to_all_metrics() {
        const item = createTemporaryObject(fixture, this);
        item.stats.height = 240;
        const scroll = findChild(item.stats, "videoStatsScroll");
        verify(scroll !== null);
        verify(scroll.contentHeight > scroll.height);
        scroll.contentY = scroll.contentHeight - scroll.height;
        verify(scroll.contentY > 0);
    }
    function test_pcr_estimate_preserves_sign_and_independent_availability() {
        const item = createTemporaryObject(fixture, this);
        const measured = { status: "measuring", latest_ms: 600, median_ms: 500, p95_ms: 700, samples: 30 };
        for (const pcr of [undefined, { status: "unavailable" }, { status: "waiting" }]) {
            item.stats.snapshot = { receive_latency: measured, pcr_deviation: pcr };
            compare(item.stats.metric("latency"), "600.0 ms");
            compare(item.stats.metric("pcrDeviation"), pcr?.status === "waiting" ? qsTranslate("Main", "Waiting for measurement") : "—");
            compare(item.stats.metric("pcrDistribution"), "—");
        }
        for (const value of [-200, 0, 200]) {
            item.stats.snapshot = { pcr_deviation: { status: "measuring", samples: 30, latest_ms: value, median_ms: -50, p95_ms: 100 } };
            compare(item.stats.metric("pcrDeviation"), (value > 0 ? "+" : "") + value.toFixed(1) + " ms");
            compare(item.stats.metric("pcrDistribution"), "-50.0 / +100.0 ms · " + qsTranslate("Main", "%1 samples").arg(30));
        }
        item.stats.snapshot = {};
        compare(item.stats.metric("pcrDeviation"), "—");
        compare(item.stats.metric("pcrDistribution"), "—");
    }
    function test_viewport_binding_across_loader_scope() {
        const item = createTemporaryObject(fixture, this);
        verify(item !== null);
        verify(item.stats !== null);
        compare(item.stats.viewportSize, Qt.size(640, 360));
        verify(item.stats.viewportDpr > 0);
        compare(item.stats.metric("viewport"), "640 × 360 / " + item.stats.viewportDpr);
        item.width = 800;
        item.height = 450;
        compare(item.stats.viewportSize, Qt.size(800, 450));
        compare(item.stats.metric("viewport"), "800 × 450 / " + item.stats.viewportDpr);
    }
    function test_scan_data() {
        return [
            { tag: "progressive", mode: "progressive", scan: "progressive", text: "Progressive" },
            { tag: "interleaved", mode: "interleaved", scan: "interlaced", text: "Interlaced" },
            { tag: "fields", mode: "fields", scan: "interlaced", text: "Interlaced (separate fields)" },
            { tag: "alternate", mode: "alternate", scan: "interlaced", text: "Interlaced (alternate fields)" },
            { tag: "mixed-i", mode: "mixed", scan: "interlaced", text: "Mixed · latest input: interlaced" },
            { tag: "mixed-p", mode: "mixed", scan: "progressive", text: "Mixed · latest input: progressive" },
            { tag: "mixed-wait", mode: "mixed", scan: "unknown", text: "Mixed · waiting for a frame" }
        ];
    }
    function test_scan(data) {
        const item = createTemporaryObject(fixture, this);
        item.stats.snapshot = { input: { interlace: data.mode, scan: data.scan, pixel_aspect_ratio: "4:3" } };
        compare(item.stats.metric("scan"), qsTranslate("Main", data.text) + " / 4:3");
    }
    function test_applied_processing_is_distinct_from_configuration() {
        const item = createTemporaryObject(fixture, this);
        for (const method of ["YADIF", "Linear", "OpenGL vfir", "VA-API adaptive"]) {
            item.stats.snapshot = { deinterlacer: method, deinterlace_status: "active" };
            compare(item.stats.metric("deinterlace"), qsTranslate("Main", "Active: %1").arg(method));
            item.stats.snapshot = { deinterlacer: method, deinterlace_status: "passthrough" };
            compare(item.stats.metric("deinterlace"), qsTranslate("Main", "Not applied (passthrough)"));
            compare(item.stats.metric("deinterlaceSetting"), method);
            item.stats.snapshot = { deinterlacer: method, deinterlace_status: "unknown" };
            compare(item.stats.metric("deinterlace"), "—");
            compare(item.stats.metric("scan"), "— / —");
        }
        item.stats.snapshot = { deinterlacer: "Off", deinterlace_status: "disabled" };
        compare(item.stats.metric("deinterlace"), qsTranslate("Main", "Disabled"));
    }
}
