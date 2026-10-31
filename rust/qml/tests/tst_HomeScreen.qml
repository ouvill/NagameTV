import QtQuick
import QtTest
import MinimalViewer as Viewer

TestCase {
    id: testCase
    name: "HomeScreen"
    when: windowShown
    visible: true
    width: 900
    height: 900
    Component {
        id: component
        Viewer.HomeScreen {
            property alias rows: fixture.rows
            channels: ChannelFixture { id: fixture }
            width: 900
            height: 900
            rows: [
                { index: 0, label: "1 First", band: "GR" },
                { index: 1, label: "2 Second", band: "GR" },
                { index: 2, label: "3 Third", band: "GR" },
                { index: 3, label: "4 Fourth", band: "GR" },
                { index: 4, label: "5 Fifth", band: "GR" },
                { index: 5, label: "BS One", band: "BS" }
            ]
            selected: 1
            viewingIndex: 1
            programsJson: JSON.stringify([
                { name: "Morning news", startAt: 100, duration: 100 }, { name: "Cooking", startAt: 100, duration: 100 },
                null, null, null, { name: "Satellite film", startAt: 100, duration: 100 }
            ])
            now: 150
        }
    }
    Component { id: resumable; Viewer.ContinueWatchingModel {} }
    ActionTestBackend { id: actionBackend }
    Viewer.ViewerActions { id: viewerActions; backend: actionBackend; targetWindow: testCase.Window.window }
    property var home
    SignalSpy { id: selection; signalName: "selectRequested" }
    SignalSpy { id: resume; signalName: "resumeRequested" }
    SignalSpy { id: returning; signalName: "returnRequested" }
    SignalSpy { id: boundary; signalName: "boundaryReached" }
    function init() {
        failOnWarning(/.*/);
        home = createTemporaryObject(component, testCase);
        verify(home !== null);
        verify(waitForRendering(home));
        for (const spy of [selection, resume, returning, boundary]) { spy.target = home; spy.clear(); }
    }
    function addRecordings() {
        home.continueWatching = resumable.createObject(home);
        verify(home.continueWatching.load_test(JSON.stringify([
            { recordingKey: "key-a", programName: "Evening drama", channelName: "First", progress: 0.25,
              positionMs: 900000, durationMs: 3600000, remainingMs: 2700000, sourceKind: "local", recordedId: "", videoFileId: "" },
            { recordingKey: "key-b", programName: "Travel", channelName: "", progress: 0.9,
              positionMs: 540000, durationMs: 600000, remainingMs: 60000, sourceKind: "local", recordedId: "", videoFileId: "" }
        ])));
        tryCompare(findChild(home, "homeContinue"), "count", 2);
    }
    function test_channels_come_before_recordings_and_open_on_the_watched_card() {
        addRecordings();
        const grid = findChild(home, "homeChannels");
        const recordings = findChild(home, "homeContinue");
        compare(grid.count, 5);
        tryVerify(() => recordings.visible && grid.mapToItem(home, 0, 0).y + grid.height <= recordings.mapToItem(home, 0, 0).y);
        home.focusChannels();
        tryCompare(grid, "activeFocus", true);
        compare(grid.currentIndex, 1);
        keyClick(Qt.Key_Return);
        compare(selection.signalArguments[0][0], 1);
    }
    function test_favorites_open_first_when_the_watched_channel_is_one() {
        home.rows = [
            { index: 0, label: "1 First", band: "GR" },
            { index: 1, label: "2 Second", band: "GR", favorite: true },
            { index: 5, label: "BS One", band: "BS", favorite: true }
        ];
        home.restoreBand();
        compare(home.band, "FAV");
        const grid = findChild(home, "homeChannels");
        tryCompare(grid, "count", 2);
        verify(findChild(home, "band-FAV").visible);
        home.viewingIndex = 0;
        home.selected = 0;
        home.restoreBand();
        compare(home.band, "GR");
        tryCompare(grid, "count", 2);
    }
    function test_vertical_keys_visit_every_row_and_keep_the_column() {
        addRecordings();
        home.playing = true;
        const grid = findChild(home, "homeChannels");
        const recordings = findChild(home, "homeContinue");
        const back = findChild(home, "homeReturn");
        home.focusChannels();
        tryCompare(grid, "activeFocus", true);
        keyClick(Qt.Key_Down);
        compare(grid.currentIndex, 4);
        keyClick(Qt.Key_Down);
        tryCompare(recordings, "activeFocus", true);
        compare(recordings.currentIndex, 1);
        keyClick(Qt.Key_Down);
        tryCompare(back, "activeFocus", true);
        // Up from the bar returns to the row just above, not to the top of the page.
        keyClick(Qt.Key_Up);
        tryCompare(recordings, "activeFocus", true);
        keyClick(Qt.Key_Up);
        tryCompare(grid, "activeFocus", true);
        compare(grid.currentIndex, 4);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        tryCompare(findChild(home, "band-GR"), "activeFocus", true);
        keyClick(Qt.Key_Up);
        compare(boundary.count, 1);
        compare(boundary.signalArguments[0][0], Qt.Key_Up);
    }
    function test_entering_from_below_lands_on_the_playing_bar() {
        home.playing = true;
        home.enterBottom();
        tryCompare(findChild(home, "homeReturn"), "activeFocus", true);
        home.playing = false;
        home.enterBottom();
        tryCompare(findChild(home, "homeChannels"), "activeFocus", true);
    }
    function test_the_bar_stops_recordings_but_leaves_live_stop_to_its_main_button() {
        actionBackend.playing = true;
        actionBackend.recording = true;
        actionBackend.playback_action = Viewer.Player.Pause;
        actionBackend.stopRequests = 0;
        home.actions = viewerActions;
        home.playing = true;
        const stop = findChild(home, "homeStop");
        verify(stop.visible);
        verify(waitForRendering(home));
        mouseClick(stop);
        compare(actionBackend.stopRequests, 1);
        actionBackend.recording = false;
        actionBackend.playback_action = Viewer.Player.Stop;
        verify(!stop.visible);
        verify(findChild(home, "homePlayback").visible);
    }
    function test_the_bar_shows_what_plays_and_leads_back_to_it() {
        const bar = findChild(home, "homeNowPlaying");
        verify(!bar.visible);
        home.playing = true;
        home.playingChannel = "2 Second";
        home.playingProgramJson = JSON.stringify({ name: "Cooking", startAt: 100, duration: 100 });
        verify(bar.visible);
        compare(findChild(bar, "homeNowPlayingChannel").text, "Second");
        compare(findChild(bar, "cardProgramTitle").text, "Cooking");
        verify(!findChild(bar, "homeNowPlayingTitle").visible);
        mouseClick(findChild(bar, "homeReturn"));
        compare(returning.count, 1);
        mouseClick(findChild(bar, "homePreview"));
        compare(returning.count, 2);
        // A file without program data is named instead.
        home.playingProgramJson = "null";
        home.playingTitle = "holiday.ts";
        verify(findChild(bar, "homeNowPlayingTitle").visible);
        compare(findChild(bar, "homeNowPlayingTitle").text, "holiday.ts");
    }
}
