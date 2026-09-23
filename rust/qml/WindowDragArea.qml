import QtQuick

MouseArea {
    id: area
    required property Window targetWindow
    signal activity
    acceptedButtons: Qt.LeftButton
    property point pressPosition: Qt.point(0, 0)
    property bool moveStarted: false
    onPressed: function(mouse) {
        pressPosition = Qt.point(mouse.x, mouse.y);
        moveStarted = false;
    }
    onReleased: moveStarted = false
    onCanceled: moveStarted = false
    onDoubleClicked: {
        if (area.targetWindow.visibility !== Window.FullScreen)
            area.targetWindow.visibility === Window.Maximized ? area.targetWindow.showNormal() : area.targetWindow.showMaximized();
    }
    // Only a press accepted by this MouseArea may initiate a move. A passive
    // DragHandler can also observe a resize press and initiate a second native
    // operation after the window manager releases its resize grab.
    onPositionChanged: function(mouse) {
        if (!pressed || !(mouse.buttons & Qt.LeftButton) || moveStarted)
            return;
        if (targetWindow.visibility !== Window.Windowed && targetWindow.visibility !== Window.Maximized)
            return;
        const distance = Application.styleHints.startDragDistance;
        if (Math.abs(mouse.x - pressPosition.x) < distance && Math.abs(mouse.y - pressPosition.y) < distance)
            return;
        moveStarted = true;
        area.activity();
        if (!targetWindow.startSystemMove())
            console.warn("Could not start the system window move");
    }
}
