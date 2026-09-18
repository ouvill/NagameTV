import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer 1.0
import ".."

Item {
    ApplicationWindow {
        id: host
        visible: true
        width: 1100
        height: 480
        ActionTestBackend { id: backend }
        ViewerActions { id: actions; backend: backend; targetWindow: host; canCapture: true }
        InputContext {
            id: inputContext
            targetWindow: host
            playbackControls: backend.recording || backend.timeshift
        }
        ShortcutBindings { actions: actions; inputContext: inputContext }
        PlayerControls {
            id: controls
            x: 24; y: 200
            width: host.width - 48
            height: implicitHeight
            actions: actions
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
        SignalSpy { id: capture; target: actions; signalName: "captureRequested" }
        SignalSpy { id: comments; target: actions; signalName: "composerVisibilityRequested" }
        TestCase {
            name: "PlayerControls"
            when: windowShown
            function init() {
                failOnWarning(/.*/);
                backend.playback_action = Player.Play;
                backend.playbackRequests = 0;
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
                actions.canCapture = true;
                navigation.guideEnabled = true;
                modes.clear(); capture.clear(); comments.clear();
            }
            function test_buttons_follow_rust_action_without_deciding_playback_mode() {
                const play = findChild(controls, "playStopButton");
                for (const mode of [false, true]) {
                    backend.recording = mode;
                    for (const state of [Player.Play, Player.Pause, Player.Stop]) {
                        backend.playback_action = state;
                        const before = backend.playbackRequests;
                        mouseClick(play);
                        compare(backend.playbackRequests, before + 1);
                        const icon = state === Player.Play ? "play-outline.svg" : state === Player.Pause ? "pause.svg" : "square.svg";
                        verify(String(play.iconSource).endsWith(icon));
                    }
                }
                backend.playback_action = Player.Unavailable;
                const before = backend.playbackRequests;
                mouseClick(play);
                compare(backend.playbackRequests, before);
            }
            function test_button_and_space_share_action_and_text_focus_only_blocks_key() {
                backend.recording = true;
                backend.playback_action = Player.Pause;
                host.requestActivate();
                tryCompare(host, "active", true);
                const play = findChild(controls, "playStopButton");
                play.forceActiveFocus();
                keyClick(Qt.Key_Space);
                compare(backend.playbackRequests, 1);
                mouseClick(play);
                compare(backend.playbackRequests, 2);
                const editor = createTemporaryQmlObject('import QtQuick.Controls; TextField {}', host.contentItem);
                editor.forceActiveFocus();
                keyClick(Qt.Key_Space);
                compare(backend.playbackRequests, 2);
                verify(play.enabled);
                mouseClick(play);
                compare(backend.playbackRequests, 3);
            }
            function test_actions_follow_backend_and_recording_disables_live_comments() {
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
                mouseClick(findChild(controls, "screenshotButton"));
                compare(capture.count, 1);
                actions.canCapture = false;
                mouseClick(findChild(controls, "screenshotButton"));
                compare(capture.count, 1);
            }
            function test_recording_and_timeshift_skip_buttons_use_shared_steps() {
                backend.playing = true;
                for (const recording of [false, true]) {
                    backend.recording = recording;
                    backend.timeshift = !recording;
                    waitForRendering(controls);
                    mouseClick(findChild(controls, "skipBackButton"));
                    mouseClick(findChild(controls, "skipForwardButton"));
                }
                compare(backend.skips, [actions.seekSteps.backwardMilliseconds, actions.seekSteps.forwardMilliseconds,
                    actions.seekSteps.backwardMilliseconds, actions.seekSteps.forwardMilliseconds]);
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
