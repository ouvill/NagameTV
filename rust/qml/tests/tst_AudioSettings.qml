import QtQuick
import QtTest
import ".."

TestCase {
    id: testCase
    name: "AudioSettings"
    when: windowShown
    width: 640
    height: 600
    visible: true
    AudioSettings {
        id: popup
        windowWidth: 640
        windowHeight: 600
        iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
    }
    SignalSpy { id: refresh; target: popup; signalName: "refreshRequested" }
    SignalSpy { id: selection; target: popup; signalName: "selectRequested" }
    function test_main_labels_and_duplicate_disambiguation() {
        popup.tracksJson = JSON.stringify([
            {id: "a", language: "jpn", role: "main"},
            {id: "b", language: "eng", role: "sub"},
            {id: "c", language: "jpn", role: "both"}
        ]);
        compare(popup.trackLabel(popup.tracks[0], 0), "日本語 · 主音声");
        compare(popup.trackLabel(popup.tracks[1], 1), "English · 副音声");
        compare(popup.trackLabel(popup.tracks[2], 2), "主／副");
        popup.tracksJson = JSON.stringify([
            {id: "a", language: "jpn", role: "main"},
            {id: "b", language: "jpn", role: "main"}
        ]);
        compare(popup.trackLabel(popup.tracks[1], 1), "日本語 · 主音声 · 音声 2");
        compare(popup.trackLabel({id: "c", language: "", role: ""}, 2), "音声 3");
        popup.tracksJson = "[]";
    }
    function test_identity_confirmation_and_closed_updates() {
        failOnWarning(/.*/);
        popup.open();
        tryCompare(popup, "opened", true);
        verify(refresh.count > 0);
        popup.playing = true;
        popup.tracksJson = JSON.stringify([
            {id: "source/audio-b", language: "jpn", title: "<b>B</b>", selected: true},
            {id: "source/audio-a", language: "eng", title: "A", selected: false}
        ]);
        const option = findChild(popup.contentItem, "audioOption1");
        verify(option !== null);
        verify(waitForRendering(option));
        mouseClick(option);
        compare(selection.count, 1);
        compare(selection.signalArguments[0][0], "source/audio-a");
        compare(popup.tracks[1].selected, false);
        compare(option.contentItem.textFormat, Text.PlainText);
        popup.errorText = "<b>音声トラックが更新されています</b>";
        const error = findChild(popup.contentItem, "audioError");
        compare(error.visible, true);
        compare(error.text, popup.errorText);
        compare(error.textFormat, Text.PlainText);
        popup.errorText = "";
        compare(error.visible, false);
        popup.playing = false;
        compare(option.enabled, false);
        popup.close();
        tryCompare(popup, "visible", false);
        compare(popup.tracks.length, 0);
        const count = refresh.count;
        wait(1100);
        compare(refresh.count, count);
    }
}
