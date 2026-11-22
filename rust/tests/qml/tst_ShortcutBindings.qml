import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer as Viewer
import MinimalViewer 1.0

Item {
    ApplicationWindow {
        id: host
        visible: true
        width: 640
        height: 480
        TestCase {
            id: testCase
            name: "ShortcutBindings"
            when: windowShown
            visible: true
            width: 640
            height: 480
            Component {
                id: component
                Item {
                    width: 600
                    height: 400
                    property alias actions: actions
                    property alias editor: editor
                    property alias navigationField: navigationField
                    property alias popup: popup
                    property alias drawer: drawer
                    property alias backend: backend
                    property alias context: inputContext
                    property alias bindings: bindings
                    readonly property bool guideRequests: backend.guide_visible
                    property int channelRequests: 0
                    readonly property int steps: backend.channelSteps
                    readonly property int dismissals: escapeSpy.count + backSpy.count
                    readonly property int commentRequests: commentSpy.count
                    property int screenshots: 0
                    property int libraryCloses: 0
                    ActionTestBackend { id: backend }
                    Viewer.ViewerActions {
                        id: actions
                        backend: backend
                        targetWindow: host
                        canCapture: true
                        onCaptureRequested: parent.screenshots++
                        onLibraryCloseRequested: { parent.libraryCloses++; libraryVisible = false; }
                        onChannelsVisibilityRequested: function(visible) { parent.channelRequests++; }
                    }
                    SignalSpy { id: escapeSpy; target: actions.dismissTopmost; signalName: "triggered" }
                    SignalSpy { id: backSpy; target: actions.goBack; signalName: "triggered" }
                    SignalSpy { id: commentSpy; target: actions; signalName: "composerVisibilityRequested" }
                    Viewer.InputContext {
                        id: inputContext
                        videoItem: parent
                        targetWindow: host
                        enabled: actions.enabled
                        playbackControls: backend.recording || backend.pausable
                        guideVisible: actions.backend.guide_visible
                        libraryVisible: actions.libraryVisible
                        channelsVisible: actions.channelsVisible
                    }
                    property var digits: []
                    property var colours: []
                    Viewer.ShortcutBindings {
                        id: bindings
                        actions: actions
                        inputContext: inputContext
                        onDigitPressed: function(digit) { parent.digits = parent.digits.concat([digit]); }
                        onColourPressed: function(key) { parent.colours = parent.colours.concat([key]); }
                    }
                    Drawer { id: drawer; width: 200; height: 300; focus: true }
                    TextField {
                        id: editor
                        width: 300
                        y: 20
                    }
                    NavigationField { id: navigationField; width: 300; y: 70 }
                    Popup {
                        id: popup
                        width: 200
                        height: 100
                        modal: true
                        focus: true
                        closePolicy: Popup.CloseOnEscape
                    }
                }
            }
            Component {
                id: keyCapture
                Item {
                    property var events: []
                    Keys.onPressed: event => events.push(["press", event.key, event.modifiers, event.isAutoRepeat])
                    Keys.onReleased: event => events.push(["release", event.key, event.modifiers, event.isAutoRepeat])
                }
            }
            property var view
            TestInputMethod { id: ime }
            // XF86 keysyms as delivered in nativeVirtualKey by Qt's Linux QPA.
            readonly property int xf86Ok: 0x10081160
            readonly property int xf86Close: 0x1008ff56
            readonly property int xf86ChannelUp: 0x10081192
            readonly property int xf86ChannelDown: 0x10081193
            readonly property int xf86Numeric0: 0x10081200
            function initTestCase() {
                failOnWarning(/.*/);
                ime.install_key_normalizer();
            }
            function init() {
                failOnWarning(/.*/);
                host.showNormal();
                tryCompare(host, "visibility", Window.Windowed);
                view = createTemporaryObject(component, testCase);
                verify(view !== null);
                host.requestActivate();
                tryCompare(host, "active", true);
                view.forceActiveFocus();
                // Back input is dispatched to the focus item, which the first
                // activation of the window establishes after it reports active.
                tryCompare(host, "activeFocusItem", view);
            }
            function cleanup() {
                host.showNormal();
                tryCompare(host, "visibility", Window.Windowed);
            }
            function dismissInputs() {
                return [{tag: "escape", key: Qt.Key_Escape}, {tag: "key-back", key: Qt.Key_Back},
                    {tag: "alt-left", key: Qt.Key_Left, modifiers: Qt.AltModifier}, {tag: "mouse-back", mouse: true}];
            }
            function dismiss(data) {
                if (data.mouse) mouseClick(view, 350, 200, Qt.BackButton);
                else keyClick(data.key, data.modifiers ?? Qt.NoModifier);
            }
            function test_closed_drawer_does_not_disable_navigation_data() { return dismissInputs(); }
            function test_closed_drawer_does_not_disable_navigation(data) {
                compare(view.drawer.visible, false);
                compare(view.context.popupOpen, false);
                keyClick(Qt.Key_S);
                compare(view.channelRequests, 1);
                view.drawer.open();
                tryCompare(view.drawer, "opened", true);
                compare(view.context.popupOpen, true);
                keyClick(Qt.Key_S);
                compare(view.channelRequests, 1);
                dismiss(data);
                tryCompare(view.drawer, "visible", false);
                compare(view.context.popupOpen, false);
                view.forceActiveFocus();
                keyClick(Qt.Key_S);
                compare(view.channelRequests, 2);
            }
            function test_shortcuts_and_editable_text() {
                keyClick(Qt.Key_S);
                compare(view.channelRequests, 1);
                keyClick(Qt.Key_G);
                compare(view.guideRequests, true);
                keyClick(Qt.Key_PageUp);
                compare(view.steps, 1);
                keyClick(Qt.Key_PageDown);
                compare(view.steps, 0);
                view.editor.forceActiveFocus();
                compare(view.context.editingText, true);
                keyClick(Qt.Key_G);
                compare(view.editor.text.toLowerCase(), "g");
                keyClick(Qt.Key_S);
                compare(view.editor.text.toLowerCase(), "gs");
                keyClick(Qt.Key_C);
                compare(view.editor.text.toLowerCase(), "gsc");
                compare(view.commentRequests, 0);
                compare(view.channelRequests, 1);
                compare(view.guideRequests, true);
                keyClick(Qt.Key_PageDown);
                compare(view.steps, 0);
                view.forceActiveFocus();
                view.actions.channelsVisible = true;
                keyClick(Qt.Key_C);
                compare(view.commentRequests, 1);
                compare(view.channelRequests, 2);
                compare(view.guideRequests, false);
                view.backend.epg_enabled = false;
                keyClick(Qt.Key_G);
                compare(view.guideRequests, false);
            }
            function test_ime_forwarded_keys_after_editing_use_the_same_bindings() {
                // Ordinary delivery initially works; IME forwarding begins
                // after a text field has received focus.
                keyClick(Qt.Key_S);
                compare(view.channelRequests, 1);
                view.editor.forceActiveFocus();
                verify(ime.compose("", "実況"));
                view.forceActiveFocus();
                ime.forward_key(Qt.Key_S, Qt.NoModifier, "s", false);
                compare(view.channelRequests, 2);
                ime.forward_key(Qt.Key_S, Qt.NoModifier, "s", true);
                compare(view.channelRequests, 2);
                ime.forward_key(Qt.Key_G, Qt.NoModifier, "g", false);
                verify(view.guideRequests);
                ime.forward_key(Qt.Key_G, Qt.NoModifier, "g", false);
                verify(!view.guideRequests);
                ime.forward_key(Qt.Key_C, Qt.NoModifier, "c", false);
                compare(view.commentRequests, 1);
                const binding = findChild(view.bindings, "channelsShortcut");
                binding.sequence = "Ctrl+K";
                ime.forward_key(Qt.Key_S, Qt.NoModifier, "s", false);
                compare(view.channelRequests, 2);
                ime.forward_key(Qt.Key_K, Qt.ControlModifier, "k", false);
                compare(view.channelRequests, 3);
                view.editor.forceActiveFocus();
                view.editor.text = "";
                ime.forward_key(Qt.Key_S, Qt.NoModifier, "s", false);
                ime.forward_key(Qt.Key_C, Qt.NoModifier, "c", false);
                ime.forward_key(Qt.Key_G, Qt.NoModifier, "g", false);
                compare(view.editor.text, "scg");
                compare(view.channelRequests, 3);
                compare(view.commentRequests, 1);
                verify(!view.guideRequests);
            }
            function test_ime_forwarded_keys_respect_popup_and_shutdown() {
                view.popup.open();
                tryCompare(view.popup, "opened", true);
                ime.forward_key(Qt.Key_S, Qt.NoModifier, "s", false);
                compare(view.channelRequests, 0);
                view.popup.close();
                tryCompare(view.popup, "visible", false);
                view.forceActiveFocus();
                view.actions.enabled = false;
                ime.forward_key(Qt.Key_S, Qt.NoModifier, "s", false);
                ime.forward_key(Qt.Key_C, Qt.NoModifier, "c", false);
                ime.forward_key(Qt.Key_G, Qt.NoModifier, "g", false);
                compare(view.channelRequests, 0);
                compare(view.commentRequests, 0);
                verify(!view.guideRequests);
            }
            function test_digits_reach_the_station_buttons_only_while_tuning_is_possible() {
                keyClick(Qt.Key_5);
                compare(view.digits, []);
                view.bindings.digitsAvailable = true;
                keyClick(Qt.Key_5);
                ime.forward_key(Qt.Key_1, Qt.NoModifier, "1", false);
                compare(view.digits, [5, 1]);
                view.editor.forceActiveFocus();
                keyClick(Qt.Key_7);
                compare(view.digits, [5, 1]);
                compare(view.editor.text, "7");
            }
            function test_escape_belongs_to_popup_before_window_data() { return dismissInputs(); }
            function test_escape_belongs_to_popup_before_window(data) {
                view.popup.open();
                tryCompare(view.popup, "opened", true);
                compare(view.context.popupOpen, true);
                keyClick(Qt.Key_G);
                compare(view.guideRequests, false);
                keyClick(Qt.Key_C);
                compare(view.commentRequests, 0);
                dismiss(data);
                tryCompare(view.popup, "opened", false);
                compare(view.dismissals, 0);
                view.forceActiveFocus();
                dismiss(data);
                compare(view.dismissals, 1);
            }
            function test_screenshot_shortcut_respects_playback_and_modal_state() {
                keyClick(Qt.Key_S, Qt.ControlModifier);
                compare(view.screenshots, 1);
                view.actions.canCapture = false;
                keyClick(Qt.Key_S, Qt.ControlModifier);
                compare(view.screenshots, 1);
                view.actions.canCapture = true;
                view.editor.forceActiveFocus();
                keyClick(Qt.Key_S, Qt.ControlModifier);
                compare(view.screenshots, 1);
                view.forceActiveFocus();
                view.popup.open();
                tryCompare(view.popup, "opened", true);
                keyClick(Qt.Key_S, Qt.ControlModifier);
                compare(view.screenshots, 1);
                view.popup.close();
                tryCompare(view.popup, "visible", false);
                view.forceActiveFocus();
                keyClick(Qt.Key_S, Qt.ControlModifier);
                compare(view.screenshots, 2);
            }
            function test_rebinding_moves_dispatch_without_changing_action() {
                const binding = findChild(view.bindings, "channelsShortcut");
                binding.sequence = "Ctrl+K";
                keyClick(Qt.Key_S);
                compare(view.channelRequests, 0);
                keyClick(Qt.Key_K, Qt.ControlModifier);
                compare(view.channelRequests, 1);
                view.actions.openChannels.trigger();
                compare(view.channelRequests, 2);
            }
            function test_media_stop_key_stops_any_playback() {
                keyClick(Qt.Key_MediaStop);
                compare(view.backend.stopRequests, 0);
                view.backend.recording = true;
                view.backend.playing = true;
                keyClick(Qt.Key_MediaStop);
                compare(view.backend.stopRequests, 1);
            }
            function test_media_play_and_pause_are_separate_operations() {
                keyClick(Qt.Key_MediaPlay);
                keyClick(Qt.Key_MediaPause);
                compare(view.backend.playRequests, 0);
                compare(view.backend.pauseRequests, 0);
                view.backend.recording = true;
                view.backend.playback_action = Player.Play;
                keyClick(Qt.Key_MediaPause);
                compare(view.backend.pauseRequests, 0);
                keyClick(Qt.Key_MediaPlay);
                verify(ime.forward_key(Qt.Key_MediaPlay, Qt.NoModifier, "", true));
                compare(view.backend.playRequests, 1);
                view.backend.playback_action = Player.Pause;
                keyClick(Qt.Key_MediaPlay);
                compare(view.backend.playRequests, 1);
                keyClick(Qt.Key_MediaPause);
                verify(ime.forward_key(Qt.Key_MediaPause, Qt.NoModifier, "", true));
                compare(view.backend.pauseRequests, 1);
                compare(view.backend.playbackRequests, 0);
                view.editor.forceActiveFocus();
                keyClick(Qt.Key_MediaPause);
                compare(view.backend.pauseRequests, 1);
            }
            function test_media_seek_repeats_only_in_seek_scope() {
                keyClick(Qt.Key_AudioRewind);
                compare(view.backend.skips, []);
                view.backend.recording = true;
                view.backend.playing = true;
                keyClick(Qt.Key_AudioRewind);
                keyClick(Qt.Key_AudioForward);
                verify(ime.forward_key(Qt.Key_AudioForward, Qt.NoModifier, "", true));
                compare(view.backend.skips, [-10000, 10000, 10000]);
                view.actions.channelsVisible = true;
                keyClick(Qt.Key_AudioRewind);
                keyClick(Qt.Key_AudioForward);
                compare(view.backend.skips, [-10000, 10000, 10000]);
            }
            function test_colours_only_send_once_while_data_broadcast_presents() {
                const keys = [Qt.Key_Red, Qt.Key_Green, Qt.Key_Yellow, Qt.Key_Blue];
                for (const key of keys) keyClick(key);
                compare(view.colours, []);
                view.bindings.coloursAvailable = true;
                view.editor.forceActiveFocus();
                for (const key of keys) {
                    keyClick(key);
                    verify(ime.forward_key(key, Qt.NoModifier, "", true));
                }
                compare(view.colours, ["r", "g", "y", "b"]);
                view.bindings.coloursAvailable = false;
                for (const key of keys) keyClick(key);
                compare(view.colours, ["r", "g", "y", "b"]);
            }
            function test_native_keysym_mapping_data() {
                const cases = [
                    {tag: "ok", keysym: xf86Ok, original: 0, key: Qt.Key_Return},
                    {tag: "close", keysym: xf86Close, original: Qt.Key_Close, key: Qt.Key_Back},
                    {tag: "channel-up", keysym: xf86ChannelUp, original: 0, key: Qt.Key_PageUp},
                    {tag: "channel-down", keysym: xf86ChannelDown, original: 0, key: Qt.Key_PageDown},
                    {tag: "unmapped-close", keysym: 0, original: Qt.Key_Close, key: Qt.Key_Close},
                    {tag: "outside-numeric-range", keysym: xf86Numeric0 + 10, original: Qt.Key_A, key: Qt.Key_A}
                ];
                for (let digit = 0; digit <= 9; digit++)
                    cases.push({tag: "digit-" + digit, keysym: xf86Numeric0 + digit, original: 0, key: Qt.Key_0 + digit});
                return cases;
            }
            function test_native_keysym_mapping(data) {
                if (Qt.platform.os !== "linux") skip("Linux XKB keysyms");
                view.bindings.enabled = false;
                const focusTarget = createTemporaryObject(keyCapture, view);
                verify(focusTarget !== null);
                focusTarget.forceActiveFocus();
                const modifiers = Qt.ControlModifier | Qt.ShiftModifier;
                ime.native_key(data.original, data.keysym, modifiers, false, false);
                ime.native_key(data.original, data.keysym, modifiers, true, false);
                ime.native_key(data.original, data.keysym, modifiers, false, true);
                compare(focusTarget.events, [
                    ["press", data.key, modifiers, false],
                    ["press", data.key, modifiers, true],
                    ["release", data.key, modifiers, false]
                ]);
            }
            function test_normalized_keys_reach_focused_controls_and_shortcuts_once() {
                if (Qt.platform.os !== "linux") skip("Linux XKB keysyms");
                view.bindings.digitsAvailable = true;
                ime.native_key(0, xf86ChannelUp, Qt.NoModifier, false, false);
                compare(view.steps, 1);
                ime.native_key(0, xf86ChannelDown, Qt.NoModifier, false, false);
                compare(view.steps, 0);
                ime.native_key(0, xf86Numeric0 + 5, Qt.NoModifier, false, false);
                ime.native_key(0, xf86Numeric0 + 5, Qt.NoModifier, true, false);
                compare(view.digits, [5]);
                view.navigationField.focusForNavigation();
                ime.native_key(0, xf86Ok, Qt.NoModifier, false, false);
                compare(view.navigationField.interaction, NavigationField.Editing);
                ime.native_key(Qt.Key_Close, xf86Close, Qt.NoModifier, false, false);
                compare(view.navigationField.interaction, NavigationField.Navigating);
                compare(view.dismissals, 0);
                view.popup.open();
                tryCompare(view.popup, "opened", true);
                ime.native_key(Qt.Key_Close, xf86Close, Qt.NoModifier, false, false);
                tryCompare(view.popup, "visible", false);
                compare(view.dismissals, 0);
            }
            function test_page_and_media_keys_move_between_chapters() {
                view.backend.recording = true;
                view.backend.playing = true;
                keyClick(Qt.Key_PageDown);
                keyClick(Qt.Key_Forward);
                compare(view.backend.chapterSeeks, []);
                view.backend.chapters_data = JSON.stringify([{startMs: 0, name: "CM?", commercial: true}, {startMs: 10000, name: "A", commercial: false}, {startMs: 40000, name: "CM", commercial: true}]);
                keyClick(Qt.Key_PageDown);
                keyClick(Qt.Key_PageUp);
                keyClick(Qt.Key_MediaNext);
                keyClick(Qt.Key_MediaPrevious);
                keyClick(Qt.Key_Forward);
                verify(ime.forward_key(Qt.Key_Forward, Qt.NoModifier, "", true));
                compare(view.backend.chapterSeeks, [1, -1, 1, -1, 1]);
                compare(view.backend.skips, []);
            }
            function test_remote_dispatch_uses_enabled_bindings_and_repeat_policy() {
                compare(view.bindings.dispatch(Qt.Key_5, Qt.NoModifier, false), false);
                view.bindings.digitsAvailable = true;
                verify(view.bindings.dispatch(Qt.Key_5, Qt.NoModifier, false));
                verify(view.bindings.dispatch(Qt.Key_5, Qt.NoModifier, true));
                compare(view.digits, [5]);
                view.backend.recording = true;
                view.backend.playing = true;
                view.backend.playback_action = Player.Pause;
                verify(view.bindings.dispatch(Qt.Key_Right, Qt.NoModifier, false));
                verify(view.bindings.dispatch(Qt.Key_Right, Qt.NoModifier, true));
                compare(view.backend.skips, [10000, 10000]);
                compare(view.bindings.dispatch(Qt.Key_Blue, Qt.NoModifier, false), false);
            }
            function test_seek_yields_to_slider_and_panels_and_closing_disables_all() {
                view.backend.recording = true;
                view.backend.playing = true;
                view.backend.playback_action = Player.Pause;
                keyClick(Qt.Key_C);
                compare(view.commentRequests, 0);
                keyClick(Qt.Key_Left);
                keyClick(Qt.Key_Right);
                compare(view.backend.skips, [-10000, 10000]);
                const slider = createTemporaryQmlObject('import QtQuick.Controls; Slider { from: 0; to: 100; value: 50; stepSize: 1 }', view);
                slider.forceActiveFocus();
                keyClick(Qt.Key_Left);
                verify(slider.value < 50);
                compare(view.backend.skips.length, 2);
                const button = createTemporaryQmlObject('import QtQuick.Controls; Button { property int clicks: 0; onClicked: clicks++ }', view);
                button.forceActiveFocus();
                keyClick(Qt.Key_Right);
                keyClick(Qt.Key_Space);
                compare(view.backend.skips.length, 2);
                compare(view.backend.playbackRequests, 0);
                compare(button.clicks, 1);
                view.forceActiveFocus();
                view.actions.channelsVisible = true;
                keyClick(Qt.Key_Space);
                keyClick(Qt.Key_Right);
                compare(view.backend.playbackRequests, 0);
                compare(view.backend.skips.length, 2);
                view.actions.channelsVisible = false;
                keyClick(Qt.Key_Space);
                compare(view.backend.playbackRequests, 1);
                view.actions.enabled = false;
                keyClick(Qt.Key_Space);
                keyClick(Qt.Key_S);
                keyClick(Qt.Key_C);
                keyClick(Qt.Key_G);
                keyClick(Qt.Key_F11);
                compare(view.backend.playbackRequests, 1);
                compare(view.channelRequests, 0);
                compare(view.commentRequests, 0);
                verify(!view.backend.guide_visible);
                verify(!view.actions.fullscreen);
            }
            function test_escape_closes_guide_before_channels_data() { return dismissInputs(); }
            function test_escape_closes_guide_before_channels(data) {
                view.actions.channelsVisible = true;
                view.backend.open_screen(2);
                dismiss(data);
                verify(!view.backend.guide_visible);
                compare(view.channelRequests, 0);
                dismiss(data);
                compare(view.channelRequests, 1);
            }
            function test_library_keeps_playback_keys_local_and_escape_returns_to_viewing_data() { return dismissInputs(); }
            function test_library_keeps_playback_keys_local_and_escape_returns_to_viewing(data) {
                view.backend.recording = true;
                view.backend.playback_action = Player.Pause;
                view.actions.libraryVisible = true;
                compare(view.context.popupOpen, false);
                verify(!view.context.viewing);
                keyClick(Qt.Key_Space);
                keyClick(Qt.Key_Right);
                keyClick(Qt.Key_PageDown);
                keyClick(Qt.Key_S);
                keyClick(Qt.Key_S, Qt.ControlModifier);
                compare(view.backend.playbackRequests, 0);
                compare(view.backend.skips.length, 0);
                compare(view.steps, 0);
                compare(view.channelRequests, 0);
                compare(view.screenshots, 0);
                view.popup.open();
                tryCompare(view.popup, "opened", true);
                dismiss(data);
                tryCompare(view.popup, "visible", false);
                compare(view.libraryCloses, 0);
                view.forceActiveFocus();
                dismiss(data);
                compare(view.libraryCloses, 1);
                verify(view.context.viewing);
                keyClick(Qt.Key_Space);
                compare(view.backend.playbackRequests, 1);
            }
            function test_navigation_field_back_leaves_editing_before_library_data() { return dismissInputs(); }
            function test_navigation_field_back_leaves_editing_before_library(data) {
                view.actions.libraryVisible = true;
                view.navigationField.focusForNavigation();
                keyClick(Qt.Key_Return);
                compare(view.navigationField.interaction, NavigationField.Editing);
                verify(!view.context.accepts(InputContext.Dismiss));
                keyClick(Qt.Key_A);
                dismiss(data);
                compare(view.navigationField.interaction, NavigationField.Navigating);
                compare(view.navigationField.text, "a");
                compare(view.libraryCloses, 0);
                verify(view.context.accepts(InputContext.Dismiss));
                dismiss(data);
                compare(view.libraryCloses, 1);
            }
            function test_mouse_back_preserves_text_and_dismisses_once_per_press() {
                view.editor.text = "draft";
                view.editor.forceActiveFocus();
                mousePress(view.editor, 30, 15, Qt.BackButton, Qt.ControlModifier);
                compare(view.dismissals, 1);
                mouseRelease(view.editor, 30, 15, Qt.BackButton, Qt.ControlModifier);
                compare(view.dismissals, 1);
                compare(view.editor.text, "draft");
                mouseDoubleClickSequence(view.editor, 30, 15, Qt.BackButton);
                compare(view.dismissals, 3);
            }
            function test_alt_left_goes_back_without_seeking() {
                view.backend.recording = true;
                view.backend.playing = true;
                view.backend.playback_action = Player.Pause;
                keyPress(Qt.Key_Left, Qt.AltModifier);
                compare(view.dismissals, 1);
                // Remote-control mice may release Alt before Left.
                keyRelease(Qt.Key_Left);
                compare(view.dismissals, 1);
                compare(view.backend.skips, []);
                keyClick(Qt.Key_Left);
                compare(view.backend.skips, [-10000]);
                compare(view.dismissals, 1);
                keyClick(Qt.Key_Left, Qt.AltModifier | Qt.ShiftModifier);
                compare(view.dismissals, 1);
            }
            function test_mouse_back_respects_disabled_bindings_and_popup_policy() {
                view.bindings.enabled = false;
                mouseClick(view, 350, 200, Qt.BackButton);
                compare(view.dismissals, 0);
                view.bindings.enabled = true;
                view.actions.enabled = false;
                mouseClick(view, 350, 200, Qt.BackButton);
                compare(view.dismissals, 0);
                view.actions.enabled = true;
                view.context.enabled = false;
                mouseClick(view, 350, 200, Qt.BackButton);
                compare(view.dismissals, 0);
                view.context.enabled = true;
                view.popup.closePolicy = Popup.NoAutoClose;
                view.popup.open();
                tryCompare(view.popup, "opened", true);
                const popupEditor = createTemporaryQmlObject('import QtQuick.Controls; TextField { text: "draft" }', view.popup.contentItem);
                popupEditor.forceActiveFocus();
                mouseClick(view.popup.contentItem, 30, 30, Qt.BackButton);
                compare(view.popup.opened, true);
                compare(view.dismissals, 0);
                view.popup.closePolicy = Popup.CloseOnEscape;
                mouseClick(view.popup.contentItem, 30, 30, Qt.BackButton);
                tryCompare(view.popup, "visible", false);
                compare(view.dismissals, 0);
                compare(popupEditor.text, "draft");
            }
            function test_other_mouse_buttons_reach_controls() {
                const area = createTemporaryQmlObject('import QtQuick; MouseArea { width: 100; height: 100; acceptedButtons: Qt.AllButtons; property int clicks: 0; onClicked: clicks++ }', view);
                for (const button of [Qt.LeftButton, Qt.RightButton, Qt.MiddleButton, Qt.ForwardButton])
                    mouseClick(area, 50, 50, button);
                compare(area.clicks, 4);
                compare(view.dismissals, 0);
                mouseClick(area, 50, 50, Qt.BackButton);
                compare(area.clicks, 4);
                compare(view.dismissals, 1);
            }
            function test_back_returns_screens_before_leaving_fullscreen_data() { return dismissInputs(); }
            function test_back_returns_screens_before_leaving_fullscreen(data) {
                view.backend.open_screen(3);
                keyClick(Qt.Key_F11);
                tryCompare(host, "visibility", Window.FullScreen);
                view.actions.channelsVisible = true;
                dismiss(data);
                compare(view.channelRequests, 1);
                compare(view.backend.screen, 3);
                view.actions.channelsVisible = false;
                dismiss(data);
                compare(view.backend.screen, 0);
                compare(host.visibility, Window.FullScreen);
                dismiss(data);
                tryCompare(host, "visibility", data.key === Qt.Key_Escape ? Window.Windowed : Window.FullScreen);
            }
            function test_dismiss_in_fullscreen_data() { return dismissInputs(); }
            function test_dismiss_in_fullscreen(data) {
                keyClick(Qt.Key_F11);
                tryCompare(host, "visibility", Window.FullScreen);
                view.backend.open_screen(2);
                view.popup.open();
                tryCompare(view.popup, "opened", true);
                dismiss(data);
                tryCompare(view.popup, "visible", false);
                compare(host.visibility, Window.FullScreen);
                verify(view.backend.guide_visible);
                compare(view.dismissals, 0);
                view.forceActiveFocus();
                dismiss(data);
                verify(!view.backend.guide_visible);
                compare(host.visibility, Window.FullScreen);
                compare(view.dismissals, 1);
                dismiss(data);
                tryCompare(host, "visibility", data.key === Qt.Key_Escape ? Window.Windowed : Window.FullScreen);
                compare(view.dismissals, 2);
                if (data.key !== Qt.Key_Escape) {
                    // Repeated Back stays in fullscreen; Escape can still exit.
                    dismiss(data);
                    compare(host.visibility, Window.FullScreen);
                    keyClick(Qt.Key_Escape);
                    tryCompare(host, "visibility", Window.Windowed);
                }
            }
            function test_fullscreen_restores_window_mode() {
                const window = host;
                window.showNormal();
                tryCompare(window, "visibility", Window.Windowed);
                keyClick(Qt.Key_F11);
                tryCompare(window, "visibility", Window.FullScreen);
                compare(view.actions.fullscreen, true);
                keyClick(Qt.Key_F11);
                tryCompare(window, "visibility", Window.Windowed);
                window.showMaximized();
                tryCompare(window, "visibility", Window.Maximized);
                view.actions.toggleFullscreen.trigger();
                tryCompare(window, "visibility", Window.FullScreen);
                view.actions.leaveFullscreen();
                tryCompare(window, "visibility", Window.Maximized);
            }
        }
    }
}
