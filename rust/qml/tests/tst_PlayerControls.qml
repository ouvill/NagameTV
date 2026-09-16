import QtQuick
import QtQuick.Controls
import QtTest
import ".."

Item {
    ApplicationWindow {
        id: host
        visible: true
        width: 1100
        height: 480
        QtObject {
            id: backend
            property bool playing: false
            property bool recording: false
            property bool timeshift: false
            property bool media_active: playing || paused
            property bool paused: false
            property bool connecting: false
            property bool seekable: (recording || timeshift) && media_active
            property int selected: 0
            property bool audio_muted: false
            property real volume_level: 0.5
            property bool comments_enabled: true
            property bool danmaku_enabled: false
            property real comment_font_size: 24
            property real comment_opacity: 0.8
            property real comment_speed: 1.2
            property bool subtitles_enabled: true
            property bool subtitle_display: true
            property int saved: 0
            property var skips: []
            function play() { playing = true; paused = false; }
            function pause() { paused = true; playing = false; }
            function stop() { playing = false; paused = false; }
            function skip(milliseconds) { skips.push(milliseconds); }
            function mute(value) { audio_muted = value; }
            function volume(value) { volume_level = value; }
            function save_settings() { saved++; }
            function display_subtitles(value) { subtitle_display = value; }
            function configure_danmaku(value, size, opacity, speed) { danmaku_enabled = value; }
        }
        PlayerControls {
            id: controls
            x: 24; y: 200
            width: host.width - 48
            height: implicitHeight
            backend: backend
            canCapture: true
            iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
        }
        ModeNavigation {
            id: navigation
            anchors { right: parent.right; top: parent.top; margins: 18 }
            targetWindow: host
            mode: backend.recording ? ModeNavigation.Recording : ModeNavigation.Live
            guideEnabled: true
            iconDirectory: controls.iconDirectory
        }
        SignalSpy { id: modes; target: navigation; signalName: "modeRequested" }
        SignalSpy { id: capture; target: controls; signalName: "captureRequested" }
        SignalSpy { id: comments; target: controls; signalName: "commentRequested" }
        TestCase {
            name: "PlayerControls"
            when: windowShown
            function init() {
                failOnWarning(/.*/);
                backend.playing = false;
                backend.paused = false;
                backend.recording = false; backend.timeshift = false;
                backend.comments_enabled = true;
                backend.danmaku_enabled = false;
                backend.saved = 0;
                backend.skips = [];
                backend.subtitles_enabled = true;
                backend.subtitle_display = true;
                host.width = 1100;
                controls.width = host.width - 48;
                controls.canCapture = true;
                navigation.guideEnabled = true;
                modes.clear(); capture.clear(); comments.clear();
            }
            function test_live_timeshift_has_pause_and_skip_controls() {
                backend.timeshift = true; backend.playing = true;
                const play = findChild(controls, "playStopButton");
                verify(String(play.iconSource).endsWith("pause.svg"));
                verify(findChild(controls, "skipBackButton").visible);
                verify(findChild(controls, "skipForwardButton").visible);
                mouseClick(play);
                verify(backend.paused); verify(!backend.playing);
                mouseClick(play);
                verify(backend.playing); verify(!backend.paused);
            }
            function test_actions_follow_backend_and_recording_disables_live_comments() {
                const play = findChild(controls, "playStopButton");
                mouseClick(play);
                compare(backend.playing, true);
                mouseClick(play);
                compare(backend.playing, false);
                const toggle = findChild(controls, "danmakuButton");
                mouseClick(toggle);
                compare(backend.danmaku_enabled, true);
                compare(backend.saved, 1);
                backend.danmaku_enabled = false;
                compare(toggle.active, false);
                mouseClick(findChild(controls, "subtitlesButton"));
                compare(backend.subtitle_display, false);
                backend.recording = true;
                mouseClick(toggle);
                mouseClick(findChild(controls, "postCommentButton"));
                compare(backend.saved, 1);
                compare(comments.count, 0);
                mouseClick(play);
                compare(backend.playing, true);
                mouseClick(findChild(controls, "screenshotButton"));
                compare(capture.count, 1);
                controls.canCapture = false;
                mouseClick(findChild(controls, "screenshotButton"));
                compare(capture.count, 1);
            }
            function test_recording_pause_resume_and_skips() {
                backend.recording = true;
                waitForRendering(controls);
                const play = findChild(controls, "playStopButton");
                mouseClick(play); compare(backend.playing, true);
                mouseClick(play); compare(backend.paused, true); compare(backend.media_active, true);
                mouseClick(play); compare(backend.playing, true); compare(backend.paused, false);
                mouseClick(findChild(controls, "skipBackButton"));
                mouseClick(findChild(controls, "skipForwardButton"));
                compare(backend.skips, [-10000, 30000]);
                compare(findChild(controls, "recordingStopButton"), null);
            }
            function test_request_does_not_change_mode_before_file_selection_succeeds() {
                const recording = findChild(navigation, "recordingModeButton");
                mouseClick(recording);
                compare(modes.count, 1);
                compare(modes.signalArguments[0][0], ModeNavigation.Recording);
                compare(navigation.mode, ModeNavigation.Live);
                compare(recording.active, false);
                backend.recording = true;
                compare(recording.active, true);
                navigation.guideEnabled = false;
                mouseClick(findChild(navigation, "guideModeButton"));
                compare(modes.count, 1);
                mouseClick(findChild(navigation, "liveModeButton"));
                compare(modes.signalArguments[1][0], ModeNavigation.Live);
            }
            function test_controls_remain_reachable_with_sidebar_at_minimum_window_width() {
                for (const recording of [false, true]) {
                    backend.recording = recording;
                    const names = ["muteButton", "audioSelectionButton", "playerVolumeSlider",
                        "playStopButton", "screenshotButton", "subtitlesButton", "danmakuButton",
                        "playbackSettingsButton", "fullscreenButton", "sidePanelButton"].concat(recording
                            ? ["skipBackButton", "skipForwardButton"]
                            : ["channelsButton", "postCommentButton"]);
                    for (const width of [1392, 984, 852, 780, 779, 692, 492]) {
                        controls.width = width;
                        waitForRendering(controls);
                        const boxes = names.map(name => {
                            const item = findChild(controls, name);
                            const p = item.mapToItem(controls, 0, 0);
                            verify(p.x >= 0 && p.y >= 0, name + " starts within controls");
                            verify(p.x + item.width <= width && p.y + item.height <= controls.height,
                                name + " fits at " + width);
                            return {name: name, x: p.x, y: p.y, w: item.width, h: item.height};
                        });
                        for (let i = 0; i < boxes.length; ++i) {
                            for (let j = i + 1; j < boxes.length; ++j) {
                                const a = boxes[i], b = boxes[j];
                                verify(a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y,
                                    a.name + " overlaps " + b.name + " at " + width + ": " + JSON.stringify([a, b]));
                            }
                        }
                    }
                }
            }
        }
    }
}
