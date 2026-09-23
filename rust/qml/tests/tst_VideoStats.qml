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
}
