import QtQuick
import QtTest
import MinimalViewer as Viewer

TestCase {
    id: testCase
    name: "ChannelProgram"
    when: windowShown
    visible: true
    width: 300
    height: 180
    Component {
        id: component
        Viewer.ChannelProgram {
            width: 242
            height: 101
            program: null
            now: 0
        }
    }
    function test_boundaries_missing_and_shared_clock() {
        failOnWarning(/.*/);
        const card = createTemporaryObject(component, testCase);
        verify(card !== null);
        compare(card.progress, 0);
        card.program = {
            name: "<b>Plain title</b>",
            startAt: 100,
            duration: 100
        };
        card.now = 150;
        compare(card.progress, 0.5);
        const title = findChild(card, "cardProgramTitle");
        compare(title.textFormat, Text.PlainText);
        compare(title.text, "<b>Plain title</b>");
        card.now = 99;
        compare(card.progress, 0);
        card.now = 201;
        compare(card.progress, 1);
        card.program = null;
        compare(card.progress, 0);
        compare(title.text, qsTranslate("Viewer", "No current program information"));
    }
    function test_next_program_is_small_plain_text_and_clears_data() {
        return [{tag: "normal", emphasized: false}, {tag: "selected", emphasized: true}];
    }
    function test_next_program_is_small_plain_text_and_clears(data) {
        failOnWarning(/.*/);
        const card = createTemporaryObject(component, testCase);
        card.emphasized = data.emphasized;
        const next = findChild(card, "cardNextProgram");
        verify(!next.visible);
        const start = new Date(2026, 8, 8, 21, 0).getTime();
        card.program = {name: "Current program with a long title that spans two lines",
            startAt: start - 3600000, duration: 3600000,
            next: {name: "<b>Next show</b>", startAt: start, duration: 1800000}};
        tryCompare(next, "visible", true);
        compare(next.text, qsTranslate("Viewer", "Next %1 %2").arg("21:00").arg("<b>Next show</b>"));
        compare(next.textFormat, Text.PlainText);
        verify(next.font.pixelSize < findChild(card, "cardProgramTitle").font.pixelSize);
        wait(50);
        verify(next.y + next.height <= card.height);
        card.program = {name: "Last show", startAt: start, duration: 1800000, next: null};
        tryCompare(next, "visible", false);
        compare(next.text, "");
        card.program = null;
        verify(!next.visible);
    }

}
