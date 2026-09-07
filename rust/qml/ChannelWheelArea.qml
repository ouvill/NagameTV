import QtQuick

// One bounded step per mouse-wheel event, matching main's channel browsers.
MouseArea {
    id: root
    required property Flickable view
    required property real step
    property bool horizontal: false
    acceptedButtons: Qt.LeftButton
    propagateComposedEvents: true
    scrollGestureEnabled: false
    onPressed: function(mouse) { mouse.accepted = false; }
    onClicked: function(mouse) { mouse.accepted = false; }
    onWheel: function(event) {
        const delta = event.angleDelta.y || event.angleDelta.x;
        if (delta === 0) {
            event.accepted = false;
            return;
        }
        view.cancelFlick();
        const position = horizontal ? view.contentX : view.contentY;
        const contentSize = horizontal ? view.contentWidth : view.contentHeight;
        const viewportSize = horizontal ? view.width : view.height;
        const next = Math.max(0, Math.min(contentSize - viewportSize,
            position + (delta < 0 ? step : -step)));
        if (horizontal) view.contentX = next;
        else view.contentY = next;
        event.accepted = true;
    }
}
