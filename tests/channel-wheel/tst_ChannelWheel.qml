import QtQuick
import QtTest
import "../../rust/qml" as Viewer

TestCase {
    id: testCase
    name: "ChannelWheel"
    when: windowShown
    visible: true
    width: 960
    height: 360

    Component {
        id: browserComponent
        Viewer.ChannelBrowser {
            width: 960
            height: 304
            iconDirectory: Qt.resolvedUrl("../../assets/icons/")
        }
    }

    function test_reaches_both_ends_data() {
        return [
            { tag: "first-selected", selected: 0 },
            { tag: "middle-selected", selected: 12 },
            { tag: "last-selected", selected: 23 }
        ];
    }

    function test_reaches_both_ends(data) {
        const rows = [];
        for (let i = 0; i < 24; ++i)
            rows.push({index: i, band: "GR", label: "Channel " + i, logo: ""});
        const browser = createTemporaryObject(browserComponent, testCase,
            { rows: rows, selected: data.selected });
        verify(browser !== null);
        const list = findChild(browser, "browserList");
        verify(list !== null);
        wait(100);
        for (let i = 0; i < 90; ++i) {
            mouseWheel(list, list.width / 2, list.height / 2, 0, -120);
            wait(5);
        }
        const last = list.itemAtIndex(rows.length - 1);
        verify(last !== null, "Last channel must be instantiated");
        verify(last.x + last.width <= list.contentX + list.width + 1,
            "Last card clipped: " + JSON.stringify({right: last.x + last.width,
                viewportRight: list.contentX + list.width, origin: list.originX}));
        for (let i = 0; i < 90; ++i) {
            mouseWheel(list, list.width / 2, list.height / 2, 0, 120);
            wait(5);
        }
        const first = list.itemAtIndex(0);
        verify(first !== null);
        verify(first.x >= list.contentX - 1, "First card must be fully visible");
    }
}
