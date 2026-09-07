import QtQuick
import QtQuick.Controls
import QtTest
import ".."

Item {
    ApplicationWindow {
        id: host
        visible: true
        flags: Qt.Window | Qt.FramelessWindowHint
        width: 640
        height: 480
        property int closeRequests: 0
        onClosing: function(event) {
            closeRequests++;
            event.accepted = false;
        }
        WindowButtons {
            id: buttons
            targetWindow: host
            iconDirectory: Qt.resolvedUrl("../../../assets/icons/")
        }
        WindowDragArea {
            id: dragArea
            x: 140
            width: 300
            height: 76
            targetWindow: host
        }
        WindowResizeFrame {
            id: resizeFrame
            anchors.fill: parent
            targetWindow: host
        }
        MouseArea {
            id: otherControl
            x: dragArea.x
            y: 20
            width: dragArea.width
            height: 40
            z: 2000
            visible: false
            acceptedButtons: Qt.LeftButton
        }
        SignalSpy {
            id: moveRequests
            target: dragArea
            signalName: "activity"
        }
        TestCase {
            name: "WindowButtons"
            when: windowShown
            function init() {
                failOnWarning(/.*/);
                host.showNormal();
                tryCompare(host, "visibility", Window.Windowed);
                moveRequests.clear();
            }
            function cleanup() {
                otherControl.visible = false;
                host.showNormal();
            }
            function test_other_control_press_cannot_start_title_move() {
                otherControl.visible = true;
                mousePress(otherControl, 30, 20);
                try {
                    verify(otherControl.pressed);
                    mouseMove(otherControl, 100, 20, 20);
                    mouseMove(otherControl, 150, 20, 20);
                } finally {
                    mouseRelease(otherControl, 150, 20);
                }
                compare(moveRequests.count, 0);
                mouseMove(dragArea, 200, 30, 20);
                compare(moveRequests.count, 0);
            }
            function test_short_title_press_and_release_does_not_move() {
                mousePress(dragArea, 100, 30);
                mouseMove(dragArea, 101, 30, 20);
                mouseRelease(dragArea, 101, 30);
                mouseMove(dragArea, 220, 40, 20);
                compare(moveRequests.count, 0);
            }
            function test_title_double_click_and_fullscreen_guard() {
                mouseDoubleClickSequence(dragArea, 100, 30);
                tryCompare(host, "visibility", Window.Maximized);
                mouseDoubleClickSequence(dragArea, 100, 30);
                tryCompare(host, "visibility", Window.Windowed);
                host.showFullScreen();
                tryCompare(host, "visibility", Window.FullScreen);
                mouseDoubleClickSequence(dragArea, 100, 30);
                compare(host.visibility, Window.FullScreen);
                // Resize hit regions must not consume fullscreen/maximized input.
                for (let child of resizeFrame.children)
                    compare(child.enabled, false);
            }
            function test_maximize_restore_and_fullscreen_exit() {
                const maximize = findChild(buttons, "maximizeWindow");
                const originalWidth = host.width;
                const originalHeight = host.height;
                mouseClick(maximize);
                tryCompare(host, "visibility", Window.Maximized);
                mouseClick(maximize);
                tryCompare(host, "visibility", Window.Windowed);
                tryCompare(host, "width", originalWidth);
                tryCompare(host, "height", originalHeight);
                host.showFullScreen();
                tryCompare(host, "visibility", Window.FullScreen);
                mouseClick(maximize);
                tryCompare(host, "visibility", Window.Maximized);
            }
            function test_minimize_and_close_delivers_closing_event() {
                mouseClick(findChild(buttons, "minimizeWindow"));
                tryCompare(host, "visibility", Window.Minimized);
                host.showNormal();
                tryCompare(host, "visibility", Window.Windowed);
                const before = host.closeRequests;
                mouseClick(findChild(buttons, "closeWindow"));
                compare(host.closeRequests, before + 1);
                compare(host.visible, true);
            }
        }
    }
}
