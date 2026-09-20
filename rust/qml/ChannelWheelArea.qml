pragma ComponentBehavior: Bound
import QtQuick

// Keep the wheel detent distance while letting touchpad pixels track directly.
Item {
    id: root
    enum Unit { Pixels, Angle }
    enum Gesture { Idle, Started, PixelInput, WheelInput }
    required property Flickable view
    property int orientation: Qt.Vertical
    property bool pixelInertia: false
    readonly property real wheelStepPixels: 144
    readonly property int angleUnitsPerStep: 120 // Qt: one 15-degree wheel detent.
    readonly property real gestureIdleSeconds: pixelInertia && history.gesture === ChannelWheelArea.PixelInput ? 0.06 : 0.16
    readonly property int velocityWindowMs: 80
    readonly property int releaseToleranceMs: 40
    readonly property real minimumFlickSpeed: 160 // pixels/second; reject slow adjustments and momentum tails.
    readonly property bool active: history.gesture !== ChannelWheelArea.Idle
    readonly property bool wheelActive: verticalWheel.active || horizontalWheel.active
    signal scrollStarted()
    signal scrollFinished()

    QtObject {
        id: history
        property int gesture: ChannelWheelArea.Idle
        property var samples: []
        property real position: 0
        property real direction: 0
    }

    onWheelActiveChanged: {
        if (wheelActive) beginGesture();
        else finishGesture();
    }
    onEnabledChanged: if (!enabled) { cancelGesture(); view.cancelFlick(); }
    onVisibleChanged: if (!visible) { cancelGesture(); view.cancelFlick(); }

    function cancelGesture() {
        history.gesture = ChannelWheelArea.Idle;
        history.samples = [];
        history.position = 0;
        history.direction = 0;
    }

    function beginGesture() {
        cancelGesture();
        history.gesture = ChannelWheelArea.Started;
        // Mark input active before cancelFlick can emit movementEnded.
        scrollStarted();
        view.cancelFlick();
    }

    function recordTravel(distance: real) {
        const now = Date.now();
        const samples = history.samples;
        const last = samples.length ? samples[samples.length - 1] : null;
        // A pause, direction reversal or bound must not retain old momentum.
        if (!distance || distance * history.direction < 0
                || (last && (now < last.time || now - last.time > velocityWindowMs))) {
            samples.length = 0;
            history.position = 0;
        }
        history.direction = Math.sign(distance);
        history.position += distance;
        if (samples.length && samples[samples.length - 1].time === now)
            samples[samples.length - 1].position = history.position;
        else
            samples.push({time: now, position: history.position});
        while (samples.length > 2 && now - samples[0].time > velocityWindowMs)
            samples.shift();
    }

    function finishGesture() {
        const samples = history.samples;
        let velocity = 0;
        if (pixelInertia && history.gesture === ChannelWheelArea.PixelInput
                && samples.length > 1 && visible && enabled && !view.dragging) {
            const first = samples[0];
            const last = samples[samples.length - 1];
            const age = Date.now() - last.time;
            if (age >= 0 && age <= gestureIdleSeconds * 1000 + releaseToleranceMs)
                velocity = (last.position - first.position) * 1000 / (last.time - first.time);
        }
        // Native momentum deltas have already been applied directly. Their slow
        // tail falls below this threshold instead of starting a second coast.
        if (Math.abs(velocity) >= minimumFlickSpeed) {
            velocity = Math.max(-view.maximumFlickVelocity, Math.min(view.maximumFlickVelocity, velocity));
            if (orientation === Qt.Horizontal) view.flick(-velocity, 0);
            else view.flick(0, -velocity);
        }
        cancelGesture();
        // The owner waits for Flickable.movementEnded before snapping.
        scrollFinished();
    }
    component ScrollWheel: WheelHandler {
        target: null
        acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad
        activeTimeout: root.gestureIdleSeconds
        onWheel: function(event) {
            event.accepted = root.scroll(event.pixelDelta, event.angleDelta);
        }
    }
    // Once either axis starts a gesture it receives both axes until ScrollEnd
    // (or the no-phase timeout), so an axis change cannot deliver it twice.
    ScrollWheel { id: verticalWheel; orientation: Qt.Vertical; enabled: !horizontalWheel.active }
    ScrollWheel { id: horizontalWheel; orientation: Qt.Horizontal; enabled: !verticalWheel.active }

    function scroll(pixelDelta: point, angleDelta: point): bool {
        const unit = pixelDelta.x || pixelDelta.y ? ChannelWheelArea.Pixels : ChannelWheelArea.Angle;
        const delta = unit === ChannelWheelArea.Pixels ? pixelDelta : angleDelta;
        // Use one axis per event, including horizontal swipes with slight drift.
        const distance = Math.abs(delta.y) >= Math.abs(delta.x) ? delta.y : delta.x;
        if (!distance)
            return false;
        if (!active)
            beginGesture();
        const gesture = unit === ChannelWheelArea.Pixels ? ChannelWheelArea.PixelInput : ChannelWheelArea.WheelInput;
        if (history.gesture !== gesture) {
            history.samples = [];
            history.position = 0;
            history.direction = 0;
            history.gesture = gesture;
        }
        const pixels = unit === ChannelWheelArea.Pixels
            ? -distance : -distance / angleUnitsPerStep * wheelStepPixels;
        // Stop any selection-centering animation before changing the viewport.
        scrollStarted();
        view.cancelFlick();
        const horizontal = orientation === Qt.Horizontal;
        const position = horizontal ? view.contentX : view.contentY;
        const contentSize = horizontal ? view.contentWidth : view.contentHeight;
        const viewportSize = horizontal ? view.width : view.height;
        // Variable-size ListView delegates can shift the content origin.
        const origin = horizontal ? view.originX : view.originY;
        const end = origin + Math.max(0, contentSize - viewportSize);
        const next = Math.max(origin, Math.min(end,
            position + pixels));
        if (horizontal) view.contentX = next;
        else view.contentY = next;
        if (pixelInertia && unit === ChannelWheelArea.Pixels)
            recordTravel(next - position);
        // Consume the event so ListView cannot also scroll it.
        return true;
    }
}
