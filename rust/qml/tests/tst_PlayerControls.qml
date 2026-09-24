import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer 1.0

Item {
    ApplicationWindow {
        id: host
        visible: true
        width: 1100
        height: 480
        ActionTestBackend { id: backend }
        ViewerActions {
            id: actions; backend: backend; targetWindow: host; canCapture: true
            audioVisible: audioPopup.visible
            onAudioRequested: { controls.closeSpeed(); audioPopup.toggle(); }
            onSpeedOpened: audioPopup.close()
        }
        AudioSettings {
            id: audioPopup
            anchorItem: controls.audioAnchor
            windowWidth: host.width; windowHeight: host.height
            volumeLevel: backend.volume_level
            muted: backend.audio_muted
            iconDirectory: controls.iconDirectory
            onVolumeRequested: function(value) { backend.volume(value); }
            onMuteRequested: function(value) { backend.mute(value); }
            onSaveRequested: backend.save_settings()
        }
        InputContext {
            id: inputContext
            targetWindow: host
            playbackControls: backend.recording || backend.timeshift
        }
        ShortcutBindings { actions: actions; inputContext: inputContext }
        OverlayVisibility {
            id: overlay
            pinned: controls.screenshotHovered
            hideDelay: 60
        }
        PlayerControls {
            id: controls
            x: 24; y: host.height - height - 18
            width: host.width - 48
            height: implicitHeight
            videoWidth: width + 48
            actions: actions
            visible: overlay.controlsVisible
            iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
        }
        ModeNavigation {
            id: navigation
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
        SignalSpy { id: subtitles; target: controls; signalName: "subtitlesRequested" }
        TestCase {
            name: "PlayerControls"
            when: windowShown
            function init() {
                failOnWarning(/.*/);
                overlay.playing = false;
                backend.playback_action = Player.Play;
                backend.playbackRequests = 0;
                backend.playback_rate = 10; backend.requested_playback_rate = 10;
                backend.rateRequests = []; backend.rateAccepted = true; backend.speed_available = true;
                backend.speed_reason = ""; backend.transport_error = "";
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
                backend.media_subtitle_available = false;
                backend.subtitle_display = true;
                host.width = 1100;
                controls.width = host.width - 48;
                actions.canCapture = true;
                navigation.guideEnabled = true;
                modes.clear(); capture.clear(); comments.clear(); audio.clear(); panel.clear();
                subtitles.clear();
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
                        const icon = state === Player.Play ? "play.svg" : state === Player.Pause ? "pause.svg" : "square.svg";
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
            function test_actions_follow_backend_and_recording_allows_danmaku() {
                const toggle = findChild(controls, "danmakuButton");
                mouseClick(toggle);
                compare(backend.danmaku_enabled, true);
                compare(backend.saved, 1);
                backend.danmaku_enabled = false;
                compare(toggle.active, false);
                mouseClick(findChild(controls, "subtitlesButton"));
                compare(backend.subtitle_display, false);
                backend.recording = true;
                verify(toggle.enabled);
                mouseClick(toggle);
                compare(backend.danmaku_enabled, true);
                compare(toggle.active, true);
                compare(backend.saved, 2);
                mouseClick(toggle);
                compare(backend.danmaku_enabled, false);
                compare(toggle.active, false);
                compare(backend.saved, 3);
                verify(!findChild(controls, "postCommentButton").visible);
                compare(comments.count, 0);
                backend.comments_enabled = false;
                verify(!toggle.enabled);
                mouseClick(toggle);
                compare(backend.danmaku_enabled, false);
                compare(backend.saved, 3);
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
            function test_general_media_subtitles_open_choices_from_wide_and_narrow_controls() {
                backend.media_subtitle_available = true;
                mouseClick(findChild(controls, "subtitlesButton"));
                compare(subtitles.count, 1);
                compare(backend.subtitle_display, true);
                controls.width = 532;
                verify(waitForRendering(controls));
                const menu = findChild(controls, "playerOverflowMenu");
                mouseClick(findChild(controls, "moreControlsButton"));
                tryCompare(menu, "opened", true);
                mouseClick(findChild(menu, "overflowSubtitles"));
                compare(subtitles.count, 2);
                compare(backend.subtitle_display, true);
                tryCompare(menu, "visible", false);
            }
            function test_screenshot_hover_shows_tip_immediately_and_keeps_controls_visible() {
                const screenshot = findChild(controls, "screenshotButton");
                const tip = findChild(screenshot, "actionToolTip");
                mouseMove(screenshot, screenshot.width / 2, screenshot.height / 2);
                compare(tip.visible, true);
                compare(controls.screenshotHovered, true);
                overlay.playing = true;
                // Remain over the camera even after its tooltip times out.
                tryCompare(tip, "visible", false);
                wait(overlay.hideDelay * 2);
                compare(controls.visible, true);
                mouseClick(screenshot);
                compare(capture.count, 1);
                mouseMove(host.contentItem, 500, 150);
                compare(controls.screenshotHovered, false);
                tryCompare(controls, "visible", false);
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
                audioPopup.close();
                controls.closeSpeed();
                findChild(controls, "playerOverflowMenu").close();
            }
            function test_speed_requests_tenths_and_shows_only_confirmed_rate_on_button() {
                backend.recording = true; backend.playing = true;
                verify(waitForRendering(controls));
                const button = findChild(controls, "playbackSpeedButton");
                const popup = findChild(controls, "playbackSpeedPanel");
                mouseClick(button); tryCompare(popup, "opened", true);
                mouseClick(findChild(popup, "speedIncrease"));
                mouseClick(findChild(popup, "speedIncrease"));
                compare(backend.rateRequests, [11, 12]);
                compare(button.text, "x1.0");
                compare(popup.draft, 12);
                verify(findChild(popup, "speedStatus").visible);
                backend.playback_rate = 12;
                compare(button.text, "x1.2");
                const presets = findChild(popup, "speedPresets");
                compare(presets.count, 4);
                mouseClick(presets.itemAt(3));
                compare(backend.requested_playback_rate, 20);
                verify(!findChild(popup, "speedIncrease").enabled);
                mouseClick(findChild(popup, "speedReset"));
                compare(backend.requested_playback_rate, 10);
                const slider = findChild(popup, "speedSlider");
                slider.forceActiveFocus(); keyClick(Qt.Key_Left);
                compare(backend.requested_playback_rate, 9);
                compare(backend.skips.length, 0);
                keyClick(Qt.Key_Escape); tryCompare(popup, "visible", false);
                compare(backend.requested_playback_rate, 9);
                verify(button.activeFocus);
            }
            function test_speed_drag_commits_on_release_and_cancel_does_not_commit() {
                backend.recording = true; backend.playing = true;
                verify(waitForRendering(controls));
                const popup = findChild(controls, "playbackSpeedPanel");
                mouseClick(findChild(controls, "playbackSpeedButton")); tryCompare(popup, "opened", true);
                const slider = findChild(popup, "speedSlider");
                mousePress(slider, slider.width / 3, slider.height / 2);
                mouseMove(slider, slider.width - 12, slider.height / 2);
                compare(backend.rateRequests.length, 0);
                mouseRelease(slider, slider.width - 12, slider.height / 2);
                compare(backend.rateRequests.length, 1);
                mousePress(slider, slider.width / 2, slider.height / 2);
                mouseMove(slider, 12, slider.height / 2);
                popup.close();
                mouseRelease(slider, 12, slider.height / 2);
                tryCompare(popup, "visible", false);
                compare(backend.rateRequests.length, 1);
            }
            function test_speed_disabled_reason_failure_and_popup_exclusion() {
                backend.timeshift = true; backend.playing = true;
                const popup = findChild(controls, "playbackSpeedPanel");
                const button = findChild(controls, "playbackSpeedButton");
                mouseClick(findChild(controls, "audioSettingsButton")); tryCompare(audioPopup, "opened", true);
                mouseClick(button); tryCompare(popup, "opened", true);
                tryCompare(audioPopup, "visible", false);
                backend.rateAccepted = false; backend.transport_error = "speed rejected";
                mouseClick(findChild(popup, "speedIncrease"));
                compare(findChild(popup, "speedStatus").text, "speed rejected");
                compare(popup.draft, 10);
                backend.rateAccepted = true; backend.transport_error = "";
                mouseClick(findChild(popup, "speedIncrease"));
                compare(backend.requested_playback_rate, 11);
                backend.requested_playback_rate = backend.playback_rate;
                backend.transport_error = "speed confirmation timed out";
                compare(findChild(popup, "speedStatus").text, "speed confirmation timed out");
                backend.speed_available = false; backend.speed_reason = "live only";
                backend.transport_error = "";
                compare(findChild(popup, "speedStatus").text, "live only");
                verify(!findChild(popup, "speedSlider").enabled);
                mouseClick(findChild(controls, "audioSettingsButton")); tryCompare(audioPopup, "opened", true);
                tryCompare(popup, "visible", false);
                audioPopup.close();
                mouseClick(button); tryCompare(popup, "opened", true);
                backend.playing = false;
                tryCompare(popup, "visible", false);
                verify(!button.visible);
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
                backend.recording = true;
                mouseClick(more); tryCompare(menu, "opened", true);
                mouseClick(findChild(menu, "overflowDanmaku"));
                compare(backend.danmaku_enabled, false); compare(backend.saved, 2);
                tryCompare(menu, "visible", false);
                backend.recording = false;
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
            function test_audio_panel_opens_on_click_not_hover_and_supports_keyboard() {
                host.requestActivate(); tryCompare(host, "active", true);
                const button = findChild(controls, "audioSettingsButton");
                mouseMove(button, button.width / 2, button.height / 2);
                wait(300);
                compare(audioPopup.visible, false);
                mouseClick(button);
                tryCompare(audioPopup, "opened", true);
                compare(button.active, true);
                const slider = findChild(audioPopup, "playerVolumeSlider");
                mouseClick(findChild(audioPopup, "muteButton")); compare(backend.audio_muted, true);
                mouseClick(slider, slider.width * 0.75, slider.height / 2);
                verify(backend.volume_level > 0.5); compare(backend.saved, 1);
                // Leaving the panel does not close an operation panel.
                mouseMove(host.contentItem, 600, 400); wait(300);
                compare(audioPopup.opened, true);
                mouseClick(button); tryCompare(audioPopup, "visible", false);
                button.forceActiveFocus(); keyClick(Qt.Key_Space);
                tryCompare(audioPopup, "opened", true);
                slider.forceActiveFocus();
                const previous = backend.volume_level;
                keyClick(Qt.Key_Left); verify(backend.volume_level < previous);
                keyClick(Qt.Key_Escape); tryCompare(audioPopup, "visible", false);
                compare(backend.saved, 2); verify(button.activeFocus);
            }
            function test_repeated_audio_click_reverses_an_unfinished_close() {
                const button = findChild(controls, "audioSettingsButton");
                mouseClick(button); tryCompare(audioPopup, "opened", true);
                mouseClick(button);
                verify(audioPopup.visible && !audioPopup.opened);
                mouseClick(button);
                tryCompare(audioPopup, "opened", true);
                verify(button.active);
            }
            function test_closing_animation_does_not_steal_focus_from_outside_editor() {
                const editor = createTemporaryQmlObject('import QtQuick.Controls; TextField { x: 600; y: 300; width: 200; height: 44 }', host.contentItem);
                mouseClick(findChild(controls, "audioSettingsButton"));
                tryCompare(audioPopup, "opened", true);
                mouseClick(editor);
                tryCompare(audioPopup, "visible", false);
                verify(editor.activeFocus);
            }
            function test_audio_panel_touch_and_outside_dismissal() {
                const button = findChild(controls, "audioSettingsButton");
                touchEvent(button).press(0, button, button.width / 2, button.height / 2).commit();
                touchEvent(button).release(0, button, button.width / 2, button.height / 2).commit();
                tryCompare(audioPopup, "opened", true);
                const mute = findChild(audioPopup, "muteButton");
                touchEvent(mute).press(0, mute, mute.width / 2, mute.height / 2).commit();
                touchEvent(mute).release(0, mute, mute.width / 2, mute.height / 2).commit();
                compare(backend.audio_muted, true);
                mouseClick(host.contentItem, 600, 400);
                tryCompare(audioPopup, "visible", false);
            }
            function test_controls_fit_with_speed_button_and_sidebar_at_minimum_width() {
                for (const recording of [false, true]) {
                    backend.recording = recording; backend.timeshift = !recording; backend.playing = true;
                    for (const width of [1392, 984, 912, 911, 852, 692, 691, 532]) {
                        controls.width = width;
                        waitForRendering(controls);
                        const names = ["audioSettingsButton", "returnToLiveButton", "playbackSpeedButton", "playStopButton", "skipBackButton", "skipForwardButton",
                            "channelsButton", "postCommentButton", "screenshotButton", "subtitlesButton", "danmakuButton",
                            "fullscreenButton", "moreControlsButton", "sidePanelButton"];
                        const boxes = names.map(name => findChild(controls, name)).filter(item => item.visible).map(item => {
                            const p = item.mapToItem(controls, 0, 0);
                            verify(p.x >= 0 && p.y >= 0, item.objectName + " starts within controls");
                            verify(p.x + item.width <= width && p.y + item.height <= controls.height,
                                item.objectName + " fits at " + width);
                            if (!controls.stacked) compare(p.y + item.height / 2, controls.height / 2);
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
