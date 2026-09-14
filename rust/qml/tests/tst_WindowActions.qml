import QtQuick
import QtQuick.Controls
import QtTest
import ".." as Viewer

Item {
    ApplicationWindow {
        id: host
        visible: true
        width: 640
        height: 480
        TestCase {
            id: testCase
            name: "WindowActions"
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
                    property int guideRequests: 0
                    property int channelRequests: 0
                    property int steps: 0
                    property int escapes: 0
                    property int screenshots: 0
                    Viewer.WindowActions {
                        id: actions
                        targetWindow: host
                        guideEnabled: true
                        screenshotEnabled: true
                        onScreenshotRequested: parent.screenshots++
                        onGuideToggleRequested: parent.guideRequests++
                        onChannelsToggleRequested: parent.channelRequests++
                        onChannelStepRequested: function (offset) {
                            parent.steps += offset;
                        }
                        onEscapeRequested: parent.escapes++
                    }
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
            property int originalVisibility
            function initTestCase() {
                failOnWarning(/.*/);
            }
            function init() {
                failOnWarning(/.*/);
                originalVisibility = host.visibility;
                view = createTemporaryObject(component, testCase);
                verify(view !== null);
                host.requestActivate();
                tryCompare(host, "active", true);
                view.forceActiveFocus();
            }
            function cleanup() {
                host.visibility = originalVisibility;
            }
            function test_closed_drawer_does_not_disable_navigation() {
                compare(view.drawer.visible, false);
                compare(view.actions.popupOpen, false);
                keyClick(Qt.Key_C);
                compare(view.channelRequests, 1);
                view.drawer.open();
                tryCompare(view.drawer, "opened", true);
                compare(view.actions.popupOpen, true);
                keyClick(Qt.Key_C);
                compare(view.channelRequests, 1);
                keyClick(Qt.Key_Escape);
                tryCompare(view.drawer, "visible", false);
                compare(view.actions.popupOpen, false);
                view.forceActiveFocus();
                keyClick(Qt.Key_C);
                compare(view.channelRequests, 2);
            }
            function test_shortcuts_and_editable_text() {
                keyClick(Qt.Key_C);
                compare(view.channelRequests, 1);
                keyClick(Qt.Key_G);
                compare(view.guideRequests, 1);
                keyClick(Qt.Key_PageDown);
                compare(view.steps, 1);
                keyClick(Qt.Key_PageUp);
                compare(view.steps, 0);
                view.editor.forceActiveFocus();
                compare(view.actions.editingText, true);
                keyClick(Qt.Key_G);
                compare(view.editor.text.toLowerCase(), "g");
                keyClick(Qt.Key_C);
                compare(view.editor.text.toLowerCase(), "gc");
                compare(view.channelRequests, 1);
                compare(view.guideRequests, 1);
                keyClick(Qt.Key_PageDown);
                compare(view.steps, 0);
                view.forceActiveFocus();
                view.actions.guideEnabled = false;
                keyClick(Qt.Key_G);
                compare(view.guideRequests, 1);
            }
            function test_escape_belongs_to_popup_before_window() {
                view.popup.open();
                tryCompare(view.popup, "opened", true);
                compare(view.actions.popupOpen, true);
                keyClick(Qt.Key_G);
                compare(view.guideRequests, 0);
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
                view.actions.screenshotEnabled = false;
                keyClick(Qt.Key_S, Qt.ControlModifier);
                compare(view.screenshots, 1);
                view.actions.screenshotEnabled = true;
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
                view.actions.toggleFullscreen();
                tryCompare(window, "visibility", Window.FullScreen);
                view.actions.leaveFullscreen();
                tryCompare(window, "visibility", Window.Maximized);
            }
        }
    }
}
