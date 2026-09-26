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
