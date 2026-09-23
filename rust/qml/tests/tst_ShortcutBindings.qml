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
                    property alias popup: popup
                    property alias drawer: drawer
                    property alias backend: backend
                    property alias context: inputContext
                    property alias bindings: bindings
                    readonly property bool guideRequests: backend.guide_visible
                    property int channelRequests: 0
                    readonly property int steps: backend.channelSteps
                    readonly property int escapes: escapeSpy.count
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
                    SignalSpy { id: commentSpy; target: actions; signalName: "composerVisibilityRequested" }
                    Viewer.InputContext {
                        id: inputContext
                        targetWindow: host
                        enabled: actions.enabled
                        playbackControls: backend.recording || backend.timeshift
                        guideVisible: actions.backend.guide_visible
                        libraryVisible: actions.libraryVisible
                        channelsVisible: actions.channelsVisible
                    }
                    Viewer.ShortcutBindings { id: bindings; actions: actions; inputContext: inputContext }
                    Drawer { id: drawer; width: 200; height: 300; focus: true }
                    TextField {
                        id: editor
                        width: 300
                        y: 20
                    }
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
            property var view
            TestInputMethod { id: ime }
            function initTestCase() {
                failOnWarning(/.*/);
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
            }
            function cleanup() {
                host.showNormal();
                tryCompare(host, "visibility", Window.Windowed);
            }
            function test_closed_drawer_does_not_disable_navigation() {
                compare(view.drawer.visible, false);
                compare(view.context.popupOpen, false);
                keyClick(Qt.Key_S);
                compare(view.channelRequests, 1);
                view.drawer.open();
                tryCompare(view.drawer, "opened", true);
                compare(view.context.popupOpen, true);
                keyClick(Qt.Key_S);
                compare(view.channelRequests, 1);
                keyClick(Qt.Key_Escape);
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
                keyClick(Qt.Key_PageDown);
                compare(view.steps, 1);
                keyClick(Qt.Key_PageUp);
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
            function test_escape_belongs_to_popup_before_window() {
                view.popup.open();
                tryCompare(view.popup, "opened", true);
                compare(view.context.popupOpen, true);
                keyClick(Qt.Key_G);
                compare(view.guideRequests, false);
                keyClick(Qt.Key_C);
                compare(view.commentRequests, 0);
                keyClick(Qt.Key_Escape);
                tryCompare(view.popup, "opened", false);
                compare(view.escapes, 0);
                view.forceActiveFocus();
                keyClick(Qt.Key_Escape);
                compare(view.escapes, 1);
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
            function test_seek_yields_to_slider_and_panels_and_closing_disables_all() {
                view.backend.recording = true;
                view.backend.playing = true;
                view.backend.playback_action = Player.Pause;
                keyClick(Qt.Key_C);
                compare(view.commentRequests, 0);
                keyClick(Qt.Key_Left);
                keyClick(Qt.Key_Right);
                compare(view.backend.skips, [view.actions.seekSteps.backwardMilliseconds, view.actions.seekSteps.forwardMilliseconds]);
                const slider = createTemporaryQmlObject('import QtQuick.Controls; Slider { from: 0; to: 100; value: 50; stepSize: 1 }', view);
                slider.forceActiveFocus();
                keyClick(Qt.Key_Left);
                verify(slider.value < 50);
                compare(view.backend.skips.length, 2);
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
            function test_escape_closes_guide_before_channels() {
                view.actions.channelsVisible = true;
                view.backend.guide_visible = true;
                keyClick(Qt.Key_Escape);
                verify(!view.backend.guide_visible);
                compare(view.channelRequests, 0);
                keyClick(Qt.Key_Escape);
                compare(view.channelRequests, 1);
            }
            function test_library_keeps_playback_keys_local_and_escape_returns_to_viewing() {
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
                keyClick(Qt.Key_Escape);
                tryCompare(view.popup, "visible", false);
                compare(view.libraryCloses, 0);
                view.forceActiveFocus();
                keyClick(Qt.Key_Escape);
                compare(view.libraryCloses, 1);
                verify(view.context.viewing);
                keyClick(Qt.Key_Space);
                compare(view.backend.playbackRequests, 1);
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
