import QtQuick

// Keep the wheel detent distance while letting touchpad pixels track directly.
ChannelScrollArea {
    id: root
    required property Flickable view
    required property real step
    property bool horizontal: false
    mode: ChannelScrollArea.Continuous
    pixelsPerStep: step
    onScrolled: function(steps) {
        view.cancelFlick();
        const position = horizontal ? view.contentX : view.contentY;
        const contentSize = horizontal ? view.contentWidth : view.contentHeight;
        const viewportSize = horizontal ? view.width : view.height;
        // Variable-size ListView delegates can shift the content origin.
        const origin = horizontal ? view.originX : view.originY;
        const end = origin + Math.max(0, contentSize - viewportSize);
        const next = Math.max(origin, Math.min(end,
            position + steps * step));
        if (horizontal) view.contentX = next;
        else view.contentY = next;
    }
}
