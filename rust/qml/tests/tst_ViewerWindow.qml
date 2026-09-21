import QtQuick
import QtQuick.Controls
import QtTest
import ".."

Item {
    ViewerWindow {
        id: host
        Component.onCompleted: initializeWindow({initial: {width: 1280, height: 720}, minimum: {width: 640, height: 360}})
        ActionTestBackend { id: backend; playing: true }
        ViewerActions {
            id: actions
            backend: backend
            targetWindow: host
            audioVisible: audio.visible
            onAudioRequested: audio.toggle()
        }
        PlayerControls {
            id: controls
            anchors { left: parent.left; right: parent.right; bottom: parent.bottom; margins: 24 }
            actions: actions
            iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
        }
        AudioSettings {
            id: audio
            anchorItem: controls.audioAnchor
            windowWidth: host.viewport.width
            windowHeight: host.viewport.height
            iconDirectory: controls.iconDirectory
            onMuteRequested: function(muted) { backend.mute(muted); }
        }
        Popup {
            id: page
            parent: Overlay.overlay
            width: parent.width
            height: parent.height
            padding: 0
            modal: true
            dim: false
            Button {
                id: pageButton
                anchors { right: parent.right; bottom: parent.bottom; margins: 24 }
                text: "Close"
                onClicked: page.close()
            }
        }
        TestCase {
            name: "ViewerWindow"
            when: windowShown
            readonly property real pixelTolerance: 0.01

            function init() {
                failOnWarning(/.*/);
                backend.audio_muted = false;
                backend.requested_playback_rate = 10;
            }
            function cleanup() {
                page.close();
                audio.close();
                controls.closeSpeed();
                tryCompare(audio, "visible", false);
                tryCompare(controls, "speedVisible", false);
            }
            function resizeWindow(width, height, scale) {
                host.width = width;
                host.height = height;
                tryCompare(host, "width", width);
                tryCompare(host, "height", height);
                tryCompare(host, "uiScale", scale);
                host.update();
                verify(waitForRendering(host.contentItem), "render " + width + "x" + height);
                const corner = host.viewport.mapToItem(host.contentItem, host.viewport.width, host.viewport.height);
                fuzzyCompare(corner.x, width, pixelTolerance);
                fuzzyCompare(corner.y, height, pixelTolerance);
                compare(host.windowSizeToRemember(), Qt.size(width, height));
            }
            function clickInWindow(item) {
                const center = item.mapToItem(host.contentItem, item.width / 2, item.height / 2);
                verify(center.x > 0 && center.x < host.width);
                verify(center.y > 0 && center.y < host.height);
                mouseClick(host.contentItem, center.x, center.y);
            }
            function test_full_window_popup_tracks_resize_and_hit_targets() {
                page.open();
                tryCompare(page, "opened", true);
                for (const size of [[640, 360, 0.5], [960, 540, 0.75], [1280, 720, 1],
                                    [1440, 810, 1], [1000, 800, 0.78125], [1440, 540, 0.75],
                                    [640, 900, 0.5], [640, 360, 0.5]]) {
                    resizeWindow(size[0], size[1], size[2]);
                    const content = page.contentItem;
                    const origin = content.mapToItem(host.contentItem, 0, 0);
                    const corner = content.mapToItem(host.contentItem, content.width, content.height);
                    fuzzyCompare(origin.x, 0, pixelTolerance);
                    fuzzyCompare(origin.y, 0, pixelTolerance);
                    fuzzyCompare(corner.x, host.width, pixelTolerance);
                    fuzzyCompare(corner.y, host.height, pixelTolerance);
                    clickInWindow(pageButton);
                    tryCompare(page, "visible", false);
                    page.open();
                    tryCompare(page, "opened", true);
                }
            }
            function test_scaled_audio_and_speed_popups_remain_interactive() {
                resizeWindow(640, 360, 0.5);
                clickInWindow(controls.audioAnchor);
                tryCompare(audio, "opened", true);
                const mute = findChild(audio.contentItem, "muteButton");
                clickInWindow(mute);
                compare(backend.audio_muted, true);
                audio.close();
                tryCompare(audio, "visible", false);
                const speed = findChild(controls, "playbackSpeedButton");
                clickInWindow(speed);
                const popup = findChild(controls, "playbackSpeedPanel");
                tryCompare(popup, "opened", true);
                resizeWindow(960, 540, 0.75);
                clickInWindow(findChild(popup.contentItem, "speedIncrease"));
                compare(backend.requested_playback_rate, 11);
            }
            function test_fullscreen_and_maximized_keep_the_windowed_size() {
                const normalWidth = 800;
                const normalHeight = 500;
                resizeWindow(normalWidth, normalHeight, 0.625);
                host.showMaximized();
                tryCompare(host, "visibility", Window.Maximized);
                compare(host.windowSizeToRemember(), Qt.size(normalWidth, normalHeight));
                host.showFullScreen();
                tryCompare(host, "visibility", Window.FullScreen);
                compare(host.windowSizeToRemember(), Qt.size(normalWidth, normalHeight));
                host.showNormal();
                tryCompare(host, "visibility", Window.Windowed);
                resizeWindow(normalWidth, normalHeight, 0.625);
            }
        }
    }
}
