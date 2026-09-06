import QtQuick
import QtQuick.Controls
import QtTest
import ".."

Item {
    ApplicationWindow {
        id: host
        visible: true
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
        TestCase {
            name: "WindowButtons"
            when: windowShown
            function init() {
                failOnWarning(/.*/);
                host.showNormal();
                tryCompare(host, "visibility", Window.Windowed);
            }
            function cleanup() { host.showNormal(); }
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
