import QtQuick
import QtTest
import ".." as Viewer

TestCase {
    id: testCase
    name: "ChannelProgram"
    when: windowShown
    width: 300
    height: 180
    Component {
        id: component
        Viewer.ChannelProgram {
            width: 240
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
        compare(title.text, "現在の番組情報がありません");
    }
}
