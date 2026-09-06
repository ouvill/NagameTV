import QtQuick

MouseArea {
    id: area
    required property Window targetWindow
    signal activity
    acceptedButtons: Qt.LeftButton
    onDoubleClicked: {
        if (area.targetWindow.visibility !== Window.FullScreen)
            area.targetWindow.visibility === Window.Maximized ? area.targetWindow.showNormal() : area.targetWindow.showMaximized();
    }
    // A press can still be a click or the first half of a double click.
    // Hand off only after the platform drag threshold has been crossed.
    // Let the compositor restore/place the window in its own coordinates.
    DragHandler {
        target: null
        acceptedButtons: Qt.LeftButton
        enabled: area.targetWindow.visibility === Window.Windowed || area.targetWindow.visibility === Window.Maximized
        onActiveChanged: {
            if (active) {
                area.activity();
                if (!area.targetWindow.startSystemMove())
                    console.warn("Could not start the system window move");
            }
        }
    }
}
