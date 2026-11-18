import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer

Item {
    ApplicationWindow {
        id: host
        width: 1280; height: 720; visible: true
        ActionTestBackend { id: backend }
        ProgramSidebar {
            id: sidebar
            anchors { right: parent.right; top: parent.top; bottom: parent.bottom }
            width: 408
            targetWindow: host
            channelModel: ChannelFixture {}
            programJson: "null"
            onPageRequested: function(value) { page = value; }
            onCloseRequested: visible = false
        }
        InputContext {
            id: context
            targetWindow: host
            videoItem: host.contentItem
            sidebarFocused: sidebar.visible && sidebar.activeFocus
            playbackControls: true
        }
        ViewerActions {
            id: actions
            backend: backend
            targetWindow: host
            programVisible: sidebar.visible
            programFocused: context.sidebarFocused
            onProgramVisibilityRequested: function(value) { sidebar.visible = value; }
        }
        ShortcutBindings { actions: actions; inputContext: context }
        SignalSpy { id: remoteKeys; target: sidebar; signalName: "dataBroadcastKeyRequested" }
        SignalSpy { id: dataButton; target: sidebar; signalName: "dataBroadcastRequested" }
        TestCase {
            name: "DataBroadcastRemote"
            when: windowShown
            function init() {
                failOnWarning(/.*/);
                host.height = 720;
                host.contentItem.forceActiveFocus();
                sidebar.visible = true; sidebar.width = 408;
                sidebar.dataBroadcastAvailable = false;
                sidebar.dataBroadcastPresenting = false;
                sidebar.dataBroadcastKeys = [];
                sidebar.page = ProgramSidebar.Playback;
                backend.playing = true; backend.recording = true;
                backend.skips = []; backend.playbackRequests = 0;
                remoteKeys.clear(); dataButton.clear();
                host.requestActivate(); tryCompare(host, "active", true);
            }
            function openRemote(groups) {
                sidebar.dataBroadcastAvailable = true;
                sidebar.dataBroadcastPresenting = true;
                sidebar.dataBroadcastKeys = groups;
                verify(waitForRendering(sidebar));
                mouseClick(findChild(sidebar, "remoteSidebarTab"));
                tryCompare(sidebar, "page", ProgramSidebar.Remote);
                const remote = findChild(sidebar, "dataBroadcastRemote");
                verify(remote !== null);
                verify(waitForRendering(remote));
                return remote;
            }
            function focusButton(remote, button) {
                button.forceActiveFocus(Qt.TabFocusReason);
                tryVerify(() => {
                    const point = button.mapToItem(remote, 0, 0);
                    return point.y >= 0 && point.y + button.height <= remote.height;
                });
            }
            function test_all_keys_dispatch_and_follow_broadcast_mask() {
                verify(!findChild(sidebar, "remoteSidebarTab").visible);
                const remote = openRemote(["basic", "data-button", "numeric-tuning"]);
                const keys = ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Enter", "Backspace",
                    "b", "r", "g", "y", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
                for (const key of keys) {
                    const button = findChild(remote, "remoteKey_" + key);
                    focusButton(remote, button);
                    mouseClick(button);
                }
                compare(remoteKeys.signalArguments.map(args => args[0]), keys);
                const data = findChild(remote, "remoteDataButton");
                focusButton(remote, data);
                mouseClick(data);
                compare(dataButton.count, 1);
                verify(sidebar.visible);
                compare(sidebar.page, ProgramSidebar.Remote);
                sidebar.dataBroadcastKeys = ["data-button"];
                verify(!findChild(remote, "remoteKey_ArrowUp").enabled);
                verify(!findChild(remote, "remoteKey_0").enabled);
                verify(findChild(remote, "remoteKey_r").enabled);
                sidebar.dataBroadcastPresenting = false;
                verify(!findChild(remote, "remoteKey_r").enabled);
                mouseClick(findChild(remote, "remoteKey_r"));
                compare(remoteKeys.count, keys.length);
                verify(findChild(remote, "remoteDataButton").enabled);
                sidebar.dataBroadcastAvailable = false;
                tryCompare(sidebar, "page", ProgramSidebar.Playback);
                verify(!findChild(sidebar, "remoteSidebarTab").visible);
                tryVerify(() => findChild(sidebar, "dataBroadcastRemote") === null);
            }
            function test_keyboard_stays_in_sidebar_and_back_closes_sidebar() {
                const remote = openRemote(["basic", "data-button", "numeric-tuning"]);
                sidebar.enter();
                tryCompare(findChild(remote, "remoteDataButton"), "activeFocus", true);
                keyClick(Qt.Key_Right);
                verify(findChild(remote, "remoteKey_ArrowDown").activeFocus);
                keyClick(Qt.Key_Up);
                verify(findChild(remote, "remoteKey_Enter").activeFocus);
                keyClick(Qt.Key_Down);
                verify(findChild(remote, "remoteKey_ArrowDown").activeFocus);
                keyClick(Qt.Key_Return);
                compare(remoteKeys.signalArguments[0][0], "ArrowDown");
                keyClick(Qt.Key_Right);
                verify(findChild(remote, "remoteKey_Backspace").activeFocus);
                keyClick(Qt.Key_Down);
                verify(findChild(remote, "remoteKey_y").activeFocus);
                keyClick(Qt.Key_Left);
                verify(findChild(remote, "remoteKey_g").activeFocus);
                for (const key of ["2", "5", "8", "0"]) {
                    keyClick(Qt.Key_Down);
                    verify(findChild(remote, "remoteKey_" + key).activeFocus);
                }
                keyClick(Qt.Key_Return);
                compare(remoteKeys.signalArguments[1][0], "0");
                keyClick(Qt.Key_D);
                compare(backend.skips.length, 0);
                compare(backend.playbackRequests, 0);
                sidebar.dataBroadcastKeys = [];
                tryCompare(findChild(remote, "remoteDataButton"), "activeFocus", true);
                keyClick(Qt.Key_Return);
                compare(dataButton.count, 1);
                for (const key of [Qt.Key_Escape, Qt.Key_Back, Qt.Key_Backspace, Qt.Key_X]) {
                    sidebar.visible = true;
                    sidebar.enter();
                    keyClick(key);
                    tryCompare(sidebar, "visible", false);
                    compare(remoteKeys.count, 2);
                }
                host.contentItem.forceActiveFocus();
                sidebar.dataBroadcastKeys = ["basic"];
                keyClick(Qt.Key_Up);
                verify(context.navigationEnabled);
                compare(remoteKeys.count, 2);
                compare(dataButton.count, 1);
            }
            function test_sidebar_scrolls_to_keys_and_preserves_tab_navigation_data() {
                return [{ tag: "normal", width: 408 }, { tag: "narrow", width: 320 }];
            }
            function test_sidebar_scrolls_to_keys_and_preserves_tab_navigation(data) {
                host.height = 600;
                sidebar.width = data.width;
                const remote = openRemote(["basic", "data-button", "numeric-tuning"]);
                const zero = findChild(remote, "remoteKey_0");
                focusButton(remote, zero);
                keyClick(Qt.Key_Up);
                verify(findChild(remote, "remoteKey_8").activeFocus);
                sidebar.enterLast();
                tryCompare(zero, "activeFocus", true);
                tryVerify(() => {
                    const point = zero.mapToItem(remote, 0, 0);
                    return point.y >= 0 && point.y + zero.height <= remote.height;
                });
                verify(remote.contentItem.contentY > 0);
                keyClick(Qt.Key_Down);
                verify(findChild(sidebar, "remoteSidebarTab").activeFocus);
                keyClick(Qt.Key_Left);
                verify(findChild(sidebar, "channelsSidebarTab").activeFocus);
                keyClick(Qt.Key_Right); keyClick(Qt.Key_Return);
                verify(remote.activeFocus);
            }
        }
    }
}
