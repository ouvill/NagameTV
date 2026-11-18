import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer 1.0

Item {
    ApplicationWindow {
        id: host
        visible: true
        width: 1000
        height: 560
        TestCase {
            id: testCase
            name: "PlaybackQuickSettings"
            when: windowShown
            width: host.width
            height: host.height
            visible: true
            QtObject {
                id: backend
                property real volume: 0.5
                property bool muted: false
                property bool subtitles: false
                property string selectedTrack: "main"
                property int saves: 0
                function tracks() {
                    return JSON.stringify([
                        {id: "main", role: "main", language: "jpn", selected: selectedTrack === "main"},
                        {id: "sub", role: "sub", language: "eng", selected: selectedTrack === "sub"}]);
                }
            }
            readonly property ProgramSidebar sidebar: sidebarLoader.item as ProgramSidebar
            Loader {
                id: sidebarLoader
                x: 600
                width: 400
                height: host.height
                sourceComponent: ProgramSidebar {
                channelModel: ChannelFixture {}
                targetWindow: host
                programJson: "null"
                iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
                page: ProgramSidebar.Playback
                playing: true
                volumeLevel: backend.volume
                muted: backend.muted
                audioTracksJson: backend.tracks()
                subtitlesAvailable: true
                subtitlesShown: backend.subtitles
                onVolumeRequested: function(fraction) { backend.volume = fraction; }
                onMuteRequested: function(value) { backend.muted = value; }
                onVolumeSaveRequested: backend.saves += 1
                onAudioTrackRequested: function(id) { backend.selectedTrack = id; audioTracksJson = backend.tracks(); }
                onSubtitlesRequested: function(value) { backend.subtitles = value; }
                }
            }
            function init() {
                backend.volume = 0.5; backend.muted = false; backend.subtitles = false;
                backend.selectedTrack = "main"; backend.saves = 0;
                sidebarLoader.active = false;
                sidebarLoader.active = true;
                tryVerify(() => sidebar !== null);
                sidebar.audioTracksJson = backend.tracks();
                sidebar.audioError = "";
                host.requestActivate();
                tryCompare(host, "active", true);
                sidebar.forceActiveFocus();
                sidebar.enter();
            }
            function focused(name) {
                const item = findChild(sidebar, name);
                verify(item !== null, name);
                tryVerify(() => item.activeFocus, 3000, name + " should have focus; focus is "
                    + (host.activeFocusItem ? host.activeFocusItem.objectName : "null"));
                return item;
            }
            function test_entering_starts_at_volume_and_arrows_adjust_it() {
                const slider = findChild(findChild(sidebar, "playbackVolume"), "settingSlider");
                tryCompare(slider, "activeFocus", true);
                keyClick(Qt.Key_Right);
                compare(backend.volume, 0.55);
                keyClick(Qt.Key_Left); keyClick(Qt.Key_Left);
                compare(backend.volume, 0.45);
                keyClick(Qt.Key_Down);
                focused("playbackMuteToggle");
                verify(backend.saves >= 1, "Leaving the slider saves the adjusted volume");
            }
            function test_mute_audio_track_and_subtitles_follow_the_list_order() {
                tryCompare(findChild(findChild(sidebar, "playbackVolume"), "settingSlider"), "activeFocus", true);
                keyClick(Qt.Key_Down);
                focused("playbackMuteToggle");
                keyClick(Qt.Key_Return);
                compare(backend.muted, true);
                compare(findChild(sidebar, "playbackVolume").valueText, "Muted");
                keyClick(Qt.Key_Down);
                const track = focused("playbackAudioTrack");
                compare(track.currentText, "日本語 · Main audio");
                keyClick(Qt.Key_Return);
                tryCompare(track.popup, "opened", true);
                keyClick(Qt.Key_Down);
                keyClick(Qt.Key_Return);
                tryCompare(backend, "selectedTrack", "sub");
                tryCompare(track, "currentText", "English · Sub audio");
                tryCompare(track.popup, "visible", false);
                keyClick(Qt.Key_Down);
                focused("playbackSubtitleToggle");
                keyClick(Qt.Key_Return);
                compare(backend.subtitles, true);
            }
            function test_single_track_hides_the_choice() {
                sidebar.audioTracksJson = JSON.stringify([{id: "main", role: "main", language: "jpn", selected: true}]);
                tryCompare(findChild(findChild(sidebar, "playbackVolume"), "settingSlider"), "activeFocus", true);
                keyClick(Qt.Key_Down);
                focused("playbackMuteToggle");
                keyClick(Qt.Key_Down);
                focused("playbackSubtitleToggle");
            }
            function test_track_errors_are_shown() {
                const error = findChild(sidebar, "playbackAudioError");
                verify(!error.visible);
                sidebar.audioError = "Could not switch audio.";
                verify(error.visible);
                compare(error.text, "Could not switch audio.");
            }
        }
    }
}
