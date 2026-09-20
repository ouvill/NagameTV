pragma ComponentBehavior: Bound
import QtQuick

// Keep the wheel detent distance while letting touchpad pixels track directly.
Item {
    id: root
    enum Unit { Pixels, Angle }
    enum Gesture { Idle, Started, PixelInput, WheelInput }
    enum Axes { SingleAxis, BothAxes }
    required property Flickable view
    property int orientation: Qt.Vertical
    property int axes: ChannelWheelArea.SingleAxis
    property bool pixelInertia: false
    readonly property real wheelStepPixels: 144
    readonly property int angleUnitsPerStep: 120 // Qt: one 15-degree wheel detent.
    readonly property real gestureIdleSeconds: pixelInertia && history.gesture === ChannelWheelArea.PixelInput ? 0.06 : 0.16
    readonly property int velocityWindowMs: 80
    readonly property int releaseToleranceMs: 40
    readonly property real minimumFlickSpeed: 160 // pixels/second; reject slow adjustments and momentum tails.
    readonly property real maximumFlickSpeedPixelsPerSecond: 2200
    readonly property real flickDecelerationPixelsPerSecondSquared: 2400
    readonly property bool active: history.gesture !== ChannelWheelArea.Idle
    readonly property bool wheelActive: verticalWheel.active || horizontalWheel.active
    signal scrollStarted()
    signal scrollFinished()

    QtObject {
        id: history
        property int gesture: ChannelWheelArea.Idle
    }
    // Each axis forgets pauses, reversals and bounds independently, so reaching
    // a horizontal edge cannot discard an ongoing vertical swipe's momentum.
    component VelocityHistory: QtObject {
        property var samples: []
        property real position: 0
        property real direction: 0
        function clear() {
            samples = [];
            position = 0;
            direction = 0;
        }
        function record(distance: real) {
            const now = Date.now();
            const last = samples.length ? samples[samples.length - 1] : null;
            if (!distance || distance * direction < 0
                    || (last && (now < last.time || now - last.time > root.velocityWindowMs)))
                clear();
            direction = Math.sign(distance);
            position += distance;
            if (samples.length && samples[samples.length - 1].time === now)
                samples[samples.length - 1].position = position;
            else
                samples.push({time: now, position: position});
            while (samples.length > 2 && now - samples[0].time > root.velocityWindowMs)
                samples.shift();
        }
        function velocity(): real {
            if (samples.length < 2) return 0;
            const first = samples[0];
            const last = samples[samples.length - 1];
            const age = Date.now() - last.time;
            if (age < 0 || age > root.gestureIdleSeconds * 1000 + root.releaseToleranceMs) return 0;
            const speed = (last.position - first.position) * 1000 / (last.time - first.time);
            if (Math.abs(speed) < root.minimumFlickSpeed) return 0;
            return Math.max(-root.view.maximumFlickVelocity, Math.min(root.view.maximumFlickVelocity, speed));
        }
    }
    VelocityHistory { id: horizontalHistory }
    VelocityHistory { id: verticalHistory }

    onWheelActiveChanged: {
        if (wheelActive) beginGesture();
        else finishGesture();
    }
    onEnabledChanged: if (!enabled) { cancelGesture(); view.cancelFlick(); }
    onVisibleChanged: if (!visible) { cancelGesture(); view.cancelFlick(); }

    function cancelGesture() {
        history.gesture = ChannelWheelArea.Idle;
        horizontalHistory.clear();
        verticalHistory.clear();
    }

    function beginGesture() {
        cancelGesture();
        history.gesture = ChannelWheelArea.Started;
        // Mark input active before cancelFlick can emit movementEnded.
        scrollStarted();
        view.cancelFlick();
    }

    function finishGesture() {
        if (pixelInertia && history.gesture === ChannelWheelArea.PixelInput
                && visible && enabled && !view.dragging) {
            // Native momentum deltas already track directly; a slow tail does
            // not start a second coast.
            const vx = horizontalHistory.velocity();
            const vy = verticalHistory.velocity();
            if (vx || vy) view.flick(-vx, -vy);
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
            event.accepted = root.handleWheel(event);
        }
    }
    // Once either axis starts a gesture it receives both axes until ScrollEnd
    // (or the no-phase timeout), so an axis change cannot deliver it twice.
    ScrollWheel { id: verticalWheel; orientation: Qt.Vertical; enabled: !horizontalWheel.active }
    ScrollWheel { id: horizontalWheel; orientation: Qt.Horizontal; enabled: !verticalWheel.active }

    function handleWheel(event): bool {
        return scroll(event.pixelDelta, event.angleDelta);
    }

    function scroll(pixelDelta: point, angleDelta: point): bool {
        const unit = pixelDelta.x || pixelDelta.y ? ChannelWheelArea.Pixels : ChannelWheelArea.Angle;
        const delta = unit === ChannelWheelArea.Pixels ? pixelDelta : angleDelta;
        // Single-axis lists use the dominant input axis, including slight drift.
        const distance = Math.abs(delta.y) >= Math.abs(delta.x) ? delta.y : delta.x;
        if (!distance)
            return false;
        if (!active)
            beginGesture();
        const gesture = unit === ChannelWheelArea.Pixels ? ChannelWheelArea.PixelInput : ChannelWheelArea.WheelInput;
        if (history.gesture !== gesture) {
            horizontalHistory.clear();
            verticalHistory.clear();
            history.gesture = gesture;
        }
        const scale = unit === ChannelWheelArea.Pixels ? -1 : -wheelStepPixels / angleUnitsPerStep;
        const pixels = axes === ChannelWheelArea.BothAxes ? Qt.point(delta.x * scale, delta.y * scale)
            : orientation === Qt.Horizontal ? Qt.point(distance * scale, 0) : Qt.point(0, distance * scale);
        // Stop any selection-centering animation before changing the viewport.
        scrollStarted();
        view.cancelFlick();
        const previous = Qt.point(view.contentX, view.contentY);
        // Variable-size ListView delegates can shift the content origin.
        if (axes === ChannelWheelArea.BothAxes || orientation === Qt.Horizontal)
            view.contentX = Math.max(view.originX, Math.min(view.originX + Math.max(0, view.contentWidth - view.width), previous.x + pixels.x));
        if (axes === ChannelWheelArea.BothAxes || orientation === Qt.Vertical)
            view.contentY = Math.max(view.originY, Math.min(view.originY + Math.max(0, view.contentHeight - view.height), previous.y + pixels.y));
        if (pixelInertia && unit === ChannelWheelArea.Pixels) {
            horizontalHistory.record(view.contentX - previous.x);
            verticalHistory.record(view.contentY - previous.y);
        }
        // Consume the event so ListView cannot also scroll it.
        return true;
    }
}
