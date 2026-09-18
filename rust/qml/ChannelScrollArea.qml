import QtQuick

// Normalize distances before applying either continuous scrolling or card steps.
MouseArea {
    id: root
    enum Mode { Continuous, Steps }
    enum Unit { Pixels, Angle }
    required property int mode
    required property real pixelsPerStep
    readonly property int angleUnitsPerStep: 120 // Qt: one 15-degree wheel detent.
    readonly property int gestureIdleMs: 250
    signal scrolled(real steps)

    acceptedButtons: Qt.NoButton
    scrollGestureEnabled: true
    onEnabledChanged: reset()
    onVisibleChanged: reset()

    QtObject {
        id: gesture
        property int unit: ChannelScrollArea.Angle
        property real remainder: 0
    }
    Timer {
        id: idle
        interval: root.gestureIdleMs
        onTriggered: root.reset()
    }
    function reset() {
        gesture.remainder = 0;
        idle.stop();
    }
    function scroll(pixelDelta: point, angleDelta: point): bool {
        const unit = pixelDelta.x || pixelDelta.y ? ChannelScrollArea.Pixels : ChannelScrollArea.Angle;
        const delta = unit === ChannelScrollArea.Pixels ? pixelDelta : angleDelta;
        // Use one axis per event, including horizontal swipes with slight drift.
        const distance = Math.abs(delta.y) >= Math.abs(delta.x) ? delta.y : delta.x;
        if (!distance) {
            reset();
            return false;
        }
        const unitsPerStep = unit === ChannelScrollArea.Pixels ? pixelsPerStep : angleUnitsPerStep;
        switch (mode) {
        case ChannelScrollArea.Continuous:
            scrolled(-distance / unitsPerStep);
            break;
        case ChannelScrollArea.Steps:
            if (gesture.unit !== unit || gesture.remainder * distance < 0)
                reset();
            gesture.unit = unit;
            gesture.remainder += distance;
            const steps = Math.trunc(gesture.remainder / unitsPerStep);
            gesture.remainder -= steps * unitsPerStep;
            idle.restart();
            if (steps)
                scrolled(-steps);
            break;
        }
        // Consume sub-step input too, so ListView cannot also scroll it.
        return true;
    }
    onWheel: function(event) {
        event.accepted = scroll(event.pixelDelta, event.angleDelta);
    }
}
