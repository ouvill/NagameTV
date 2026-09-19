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
            videoWidth: width + 48
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
        SignalSpy { id: audio; target: actions; signalName: "audioRequested" }
        SignalSpy { id: panel; target: actions; signalName: "programVisibilityRequested" }
        SignalSpy { id: comments; target: actions; signalName: "composerVisibilityRequested" }
        TestCase {
            name: "PlayerControls"
            when: windowShown
            function init() {
                failOnWarning(/.*/);
                backend.playback_action = Player.Play;
                backend.playbackRequests = 0;
                backend.playing = false;
                backend.paused = false; backend.seeking = false; backend.live_delay_ms = 0; backend.liveRequests = 0;
                backend.audio_muted = false; backend.volume_level = 0.5;
                actions.enabled = true;
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
                modes.clear(); capture.clear(); comments.clear(); audio.clear(); panel.clear();
                mouseMove(host.contentItem, 500, 150);
                verify(waitForRendering(controls));
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
                verify(!findChild(controls, "postCommentButton").visible);
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
            function cleanup() {
                findChild(controls, "playerVolumePopup").close();
                findChild(controls, "playerOverflowMenu").close();
            }
            function test_live_button_tracks_backend_and_returns_without_toggling_playback() {
                backend.timeshift = true; backend.playing = true;
                const live = findChild(controls, "returnToLiveButton");
                verify(String(live.iconSource).endsWith("radio.svg"));
                backend.live_delay_ms = 60000;
                verify(waitForRendering(controls));
                verify(String(live.iconSource).endsWith("radio-off.svg"));
                mouseClick(live); compare(backend.liveRequests, 1);
                compare(backend.playbackRequests, 0);
                backend.live_delay_ms = 0; backend.paused = true;
                verify(String(live.iconSource).endsWith("radio-off.svg"));
                backend.paused = false;
                verify(String(live.iconSource).endsWith("radio.svg"));
                backend.recording = true; verify(!live.visible);
                backend.recording = false; actions.enabled = false;
                mouseClick(live); compare(backend.liveRequests, 1);
            }
            function test_overflow_uses_same_actions_and_keeps_sidebar_at_right_edge() {
                controls.width = 532;
                const more = findChild(controls, "moreControlsButton");
                const menu = findChild(controls, "playerOverflowMenu");
                waitForRendering(controls);
                mouseClick(more); tryCompare(menu, "opened", true);
                mouseClick(findChild(menu, "overflowSubtitles"));
                compare(backend.subtitle_display, false);
                tryCompare(menu, "visible", false);
                mouseClick(more); tryCompare(menu, "opened", true);
                mouseClick(findChild(menu, "overflowDanmaku"));
                compare(backend.danmaku_enabled, true); compare(backend.saved, 1);
                tryCompare(menu, "visible", false);
                mouseClick(findChild(controls, "sidePanelButton"));
                compare(panel.count, 1);
                more.forceActiveFocus(); keyClick(Qt.Key_Space);
                tryCompare(menu, "opened", true);
                keyClick(Qt.Key_Escape); tryCompare(menu, "visible", false);
                mouseClick(more); tryCompare(menu, "opened", true);
                controls.width = 1392;
                tryCompare(menu, "visible", false);
                verify(!more.visible);
            }
            function test_volume_popup_supports_hover_keyboard_and_audio_selection() {
                host.requestActivate(); tryCompare(host, "active", true);
                const mute = findChild(controls, "muteButton");
                const popup = findChild(controls, "playerVolumePopup");
                const slider = findChild(controls, "playerVolumeSlider");
                mouseMove(mute, mute.width / 2, mute.height / 2);
                tryCompare(popup, "opened", true);
                mouseClick(mute); compare(backend.audio_muted, true);
                mouseClick(slider, slider.width * 0.75, slider.height / 2);
                verify(backend.volume_level > 0.5); compare(backend.saved, 1);
                mouseClick(findChild(controls, "audioSelectionButton"));
                compare(audio.count, 1); tryCompare(popup, "visible", false);
                mouseMove(host.contentItem, 500, 150);
                mute.forceActiveFocus(); keyClick(Qt.Key_Up);
                tryCompare(popup, "opened", true); verify(slider.activeFocus);
                const previous = backend.volume_level;
                keyClick(Qt.Key_Left); verify(backend.volume_level < previous);
                keyClick(Qt.Key_Escape); tryCompare(popup, "visible", false);
            }
            function test_controls_remain_on_one_row_with_sidebar_at_minimum_width() {
                for (const recording of [false, true]) {
                    backend.recording = recording; backend.timeshift = !recording;
                    for (const width of [1392, 984, 912, 911, 852, 692, 691, 532]) {
                        controls.width = width;
                        waitForRendering(controls);
                        const names = ["muteButton", "returnToLiveButton", "playStopButton", "skipBackButton", "skipForwardButton",
                            "channelsButton", "postCommentButton", "screenshotButton", "subtitlesButton", "danmakuButton",
                            "fullscreenButton", "moreControlsButton", "sidePanelButton"];
                        const boxes = names.map(name => findChild(controls, name)).filter(item => item.visible).map(item => {
                            const p = item.mapToItem(controls, 0, 0);
                            verify(p.x >= 0 && p.y >= 0, item.objectName + " starts within controls");
                            verify(p.x + item.width <= width && p.y + item.height <= controls.height,
                                item.objectName + " fits at " + width);
                            compare(p.y + item.height / 2, controls.height / 2);
                            return {name: item.objectName, x: p.x, y: p.y, w: item.width, h: item.height};
                        });
                        const play = findChild(controls, "playStopButton");
                        fuzzyCompare(play.mapToItem(controls, play.width / 2, 0).x, width / 2, 0.5);
                        const side = findChild(controls, "sidePanelButton");
                        compare(side.mapToItem(controls, side.width, 0).x, width);
                        for (let i = 0; i < boxes.length; ++i) {
                            for (let j = i + 1; j < boxes.length; ++j) {
                                const a = boxes[i], b = boxes[j];
                                verify(a.x + a.w <= b.x || b.x + b.w <= a.x,
                                    a.name + " overlaps " + b.name + " at " + width);
                            }
                        }
                    }
                }
            }
        }
    }
}
