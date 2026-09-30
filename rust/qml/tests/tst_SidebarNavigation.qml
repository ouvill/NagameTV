import QtQuick
import QtQuick.Controls
import QtTest
import MinimalViewer

Item {
    ApplicationWindow {
        id: host
        width: 640; height: 480; visible: true
        readonly property ProgramSidebar sidebar: sidebarLoader.item as ProgramSidebar
        Loader {
            id: sidebarLoader
            anchors { top: parent.top; right: parent.right; bottom: parent.bottom }
            width: 360
            active: false
            sourceComponent: ProgramSidebar {
                targetWindow: host
                channelModel: ChannelFixture { rows: [{label: "First", band: "GR"}, {label: "Second", band: "GR"}, {label: "Satellite", band: "BS"}] }
                programJson: "null"
                onPageRequested: function(value) { page = value; }
                onDanmakuRequested: function(value) { danmakuEnabled = value; }
                onPresentationRequested: function(display, placement) { displayMode = display; placementMode = placement; }
                onDensityRequested: function(value) { densityMode = value; }
                onAdjusted: function(size, opacity, rate) { textSize = size; textOpacity = opacity; speed = rate; }
                onShadowRequested: function(value) { shadowEnabled = value; }
                onStatsRequested: function(value) { statsVisible = value; }
            }
        }
        SignalSpy { id: adjusted; target: host.sidebar; signalName: "adjusted" }
        SignalSpy { id: toggled; target: host.sidebar; signalName: "danmakuRequested" }
        TestInputMethod { id: ime }
        TestCase {
            readonly property ProgramSidebar sidebar: host.sidebar
            name: "SidebarNavigation"
            when: windowShown
            function init() {
                failOnWarning(/.*/);
                host.contentItem.forceActiveFocus();
                sidebarLoader.active = false;
                sidebarLoader.active = true;
                tryVerify(() => sidebar !== null);
                sidebar.page = ProgramSidebar.Playback;
                sidebar.commentsEnabled = true; sidebar.danmakuEnabled = false;
                sidebar.displayMode = "scroll"; sidebar.placementMode = "sequential"; sidebar.densityMode = "normal";
                sidebar.textSize = 24; sidebar.textOpacity = 1; sidebar.speed = 1;
                sidebar.statsVisible = false; sidebar.shadowEnabled = true;
                adjusted.clear(); toggled.clear();
                host.requestActivate(); tryCompare(host, "active", true);
                mouseMove(host.contentItem, 5, 5);
                verify(waitForRendering(sidebar));
            }
            function expectFocus(name) {
                const item = findChild(sidebar, name);
                verify(item !== null, "Missing control: " + name);
                tryVerify(() => item.activeFocus,
                    5000, "Expected focus: " + name + "; actual=" + host.activeFocusItem + "; page=" + sidebar.page + "; channels=" + sidebar.channelModel.count);
            }
            function verifyVisible(name) {
                const settings = findChild(sidebar, "sidebarPlaybackSettings");
                const field = findChild(settings, name);
                tryVerify(() => {
                    const point = field.mapToItem(settings, 0, 0);
                    return point.y >= 0 && point.y + field.height <= settings.height + 1;
                }, 5000, name + " must be visible");
            }
            function test_move_adjust_and_scroll_without_pointer() {
                sidebar.enter(); expectFocus("playbackDanmakuToggle");
                keyClick(Qt.Key_Return);
                compare(sidebar.danmakuEnabled, true);
                ime.forward_key(Qt.Key_Return, Qt.NoModifier, "", true);
                compare(toggled.count, 1);
                keyClick(Qt.Key_Down); expectFocus("danmakuTextSize");
                keyClick(Qt.Key_Down); expectFocus("danmakuOpacity");
                keyClick(Qt.Key_Up); expectFocus("danmakuTextSize");
                compare(adjusted.count, 0);
                keyClick(Qt.Key_Right); compare(sidebar.textSize, 25);
                compare(adjusted.count, 1);
                verifyVisible("danmakuTextSize");
                keyClick(Qt.Key_Down); expectFocus("danmakuOpacity");
                keyClick(Qt.Key_Down); expectFocus("danmakuSpeed");
                keyClick(Qt.Key_Down); expectFocus("playbackAppearanceButton");
                keyClick(Qt.Key_Down); expectFocus("playbackStatsToggle");
                keyClick(Qt.Key_Up); expectFocus("playbackAppearanceButton");
                keyClick(Qt.Key_Return);
                compare(findChild(sidebar, "sidebarPlaybackSettings").appearanceExpanded, true);
                keyClick(Qt.Key_Down); expectFocus("motion-scroll");
                keyClick(Qt.Key_Right); expectFocus("motion-pop");
                compare(sidebar.displayMode, "pop");
                keyClick(Qt.Key_Down); expectFocus("placement-sequential");
                keyClick(Qt.Key_Right); compare(sidebar.placementMode, "random");
                keyClick(Qt.Key_Down); expectFocus("density-normal");
                for (const name of ["playbackShadowToggle", "playbackStatsToggle"]) {
                    keyClick(Qt.Key_Down); expectFocus(name);
                    tryVerify(() => findChild(sidebar, "sidebarPlaybackSettings").contentItem.contentY > 0);
                    verifyVisible(name);
                }
                compare(adjusted.count, 1);
                keyClick(Qt.Key_Enter); compare(sidebar.statsVisible, true);
                keyClick(Qt.Key_Down); expectFocus("playbackSidebarTab");
                keyClick(Qt.Key_Up); expectFocus("playbackStatsToggle");
                verifyVisible("playbackStatsToggle");
            }
            function test_disabled_fields_tabs_and_header_remain_reachable() {
                sidebar.commentsEnabled = false;
                sidebar.enter(); expectFocus("playbackStatsToggle");
                keyClick(Qt.Key_Up); expectFocus("sidebarCloseButton");
                keyClick(Qt.Key_Down); expectFocus("playbackStatsToggle");
                keyClick(Qt.Key_Down); expectFocus("playbackSidebarTab");
                keyClick(Qt.Key_Right); expectFocus("programSidebarTab");
                compare(sidebar.page, ProgramSidebar.Playback);
                keyClick(Qt.Key_Return); compare(sidebar.page, ProgramSidebar.Program);
                tryVerify(() => host.activeFocusItem !== findChild(sidebar, "programSidebarTab"));
                keyClick(Qt.Key_Return); expectFocus("programSidebarTab");
                keyClick(Qt.Key_Right); expectFocus("channelsSidebarTab");
                keyClick(Qt.Key_Return); compare(sidebar.page, ProgramSidebar.Channels);
                expectFocus("sidebarChannelList");
                keyClick(Qt.Key_Up); expectFocus("band-GR");
                keyClick(Qt.Key_Up); expectFocus("sidebarCloseButton");
                keyClick(Qt.Key_Down); expectFocus("sidebarChannelList");
                const list = findChild(sidebar, "sidebarChannelList");
                for (let i = 0; i < list.count; ++i) keyClick(Qt.Key_Down);
                expectFocus("channelsSidebarTab");
                keyClick(Qt.Key_Left); keyClick(Qt.Key_Left); expectFocus("playbackSidebarTab");
                keyClick(Qt.Key_Enter); compare(sidebar.page, ProgramSidebar.Playback);
                expectFocus("playbackStatsToggle");
            }
            function test_switch_label_is_clickable() {
                const field = findChild(sidebar, "playbackDanmakuToggle");
                findChild(sidebar, "sidebarPlaybackSettings").contentItem.contentY = 0;
                mouseClick(field, 20, field.height / 2);
                compare(sidebar.danmakuEnabled, true);
                compare(toggled.count, 1);
            }
        }
    }
}
