import QtQuick
import QtTest
import MinimalViewer as Viewer

TestCase {
    id: testCase
    name: "StoppedOverview"
    when: windowShown
    visible: true
    width: 900
    height: 720
    Component {
        id: component
        Viewer.StoppedPlayback {
            property alias rows: fixture.rows
            channels: ChannelFixture { id: fixture }
            width: 900
            height: 720
            status: "Ready"
            canPlay: true
            hasChannels: true
            hasServer: true
            rows: [
                { index: 0, label: "1 First", band: "GR", logo: "" },
                { index: 1, label: "2 Second", band: "GR", logo: "" },
                { index: 2, label: "3 Third", band: "GR", logo: "" },
                { index: 3, label: "4 Fourth", band: "GR", logo: "" },
                { index: 4, label: "BS One", band: "BS", logo: "" }
            ]
            selected: 1
            programsJson: JSON.stringify([
                { name: "Morning news", startAt: 100, duration: 100, next: { name: "Weather", startAt: 200 } },
                { name: "Cooking", startAt: 100, duration: 100 }, null, null,
                { name: "Satellite film", startAt: 100, duration: 100 }
            ])
            now: 150
        }
    }
    property var panel
    SignalSpy { id: play; signalName: "playRequested" }
    SignalSpy { id: selection; signalName: "selectRequested" }
    SignalSpy { id: recordings; signalName: "recordingsRequested" }
    SignalSpy { id: openFile; signalName: "openFileRequested" }
    function init() {
        failOnWarning(/.*/);
        panel = createTemporaryObject(component, testCase);
        verify(panel !== null);
        verify(waitForRendering(panel));
        for (const spy of [play, selection, recordings, openFile]) { spy.target = panel; spy.clear(); }
    }
    function cards() {
        const grid = findChild(panel, "overviewChannels");
        const found = [];
        for (let i = 0; i < grid.count; ++i) found.push(findChild(grid.itemAtIndex(i), "overviewChannelCard"));
        return found;
    }
    function test_lists_the_selected_band_with_current_programs() {
        verify(panel.overviewVisible);
        const grid = findChild(panel, "overviewChannels");
        compare(grid.count, 4);
        compare(grid.columns, 3);
        const first = cards()[0];
        compare(findChild(first, "cardProgramTitle").text, "Morning news");
        verify(findChild(first, "cardNextProgram").visible);
        verify(cards()[1].current);
        verify(cards()[1].background.selected);
        verify(!first.current);
        verify(!first.background.selected);
        mouseClick(findChild(panel, "band-BS"));
        tryCompare(grid, "count", 1);
        compare(findChild(cards()[0], "cardProgramTitle").text, "Satellite film");
    }
    function test_actions_and_cards_request_playback() {
        mouseClick(findChild(panel, "overviewWatch"));
        compare(play.count, 1);
        mouseClick(findChild(panel, "overviewRecordings"));
        compare(recordings.count, 1);
        mouseClick(findChild(panel, "overviewOpenFile"));
        compare(openFile.count, 1);
        mouseClick(cards()[2]);
        compare(selection.count, 1);
        compare(selection.signalArguments[0][0], 2);
    }
    function test_directional_keys_move_between_actions_bands_and_cards() {
        const grid = findChild(panel, "overviewChannels");
        const watch = findChild(panel, "overviewWatch");
        findChild(panel, "overviewActions").enter();
        tryCompare(watch, "activeFocus", true);
        keyClick(Qt.Key_Right);
        tryCompare(findChild(panel, "overviewRecordings"), "activeFocus", true);
        keyClick(Qt.Key_Down);
        tryCompare(findChild(panel, "band-GR"), "activeFocus", true);
        keyClick(Qt.Key_Down);
        tryCompare(grid, "activeFocus", true);
        compare(grid.currentIndex, 1);
        keyClick(Qt.Key_Right);
        compare(grid.currentIndex, 2);
        keyClick(Qt.Key_Right);
        compare(grid.currentIndex, 2);
        keyClick(Qt.Key_Down);
        compare(grid.currentIndex, 3);
        keyClick(Qt.Key_Return);
        compare(selection.count, 1);
        compare(selection.signalArguments[0][0], 3);
        keyClick(Qt.Key_Up);
        compare(grid.currentIndex, 0);
        keyClick(Qt.Key_Up);
        tryCompare(findChild(panel, "band-GR"), "activeFocus", true);
        keyClick(Qt.Key_Up);
        tryCompare(findChild(panel, "overviewRecordings"), "activeFocus", true);
    }
    function test_other_states_keep_the_centered_prompt() {
        const loader = findChild(panel, "stoppedOverviewLoader");
        const action = findChild(panel, "stoppedAction");
        verify(loader.item !== null);
        verify(!action.visible);
        panel.loading = true;
        verify(panel.overviewVisible);
        verify(findChild(panel, "overviewLoading").visible);
        verify(!findChild(panel, "overviewWatch").enabled);
        panel.connecting = true;
        verify(!panel.overviewVisible);
        compare(loader.item, null);
        verify(findChild(panel, "stoppedLoading").visible);
        verify(!action.visible);
        panel.connecting = false;
        panel.loading = false;
        verify(panel.overviewVisible);
        panel.playbackError = "failed";
        verify(!panel.overviewVisible);
        compare(loader.item, null);
        verify(action.visible);
        panel.playbackError = "";
        panel.recording = true;
        verify(!panel.overviewVisible);
        panel.recording = false;
        panel.selected = -1;
        verify(panel.overviewVisible);
        verify(!findChild(panel, "overviewWatch").visible);
    }
}
