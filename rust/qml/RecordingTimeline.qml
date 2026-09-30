pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer
import QtQuick.Controls
import QtQuick.Layouts

ColumnLayout {
    id: root
    required property var backend
    readonly property real seekStart: Number.isFinite(backend.window_start_ms) ? backend.window_start_ms : 0
    readonly property real axisStart: seekStart
    readonly property real axisEnd: Math.max(1, seekEnd > seekStart ? seekEnd : backend.duration_ms)
    readonly property real progressStart: axisStart
    readonly property real progressEnd: backend.position_ms
    readonly property real seekTarget: backend.seek_target_ms
    readonly property RecordingSeekSteps seekSteps: RecordingSeekSteps {}
    readonly property real millisecondsPerSecond: 1000
    function fraction(time) { return Math.max(0, Math.min(1, (time - axisStart) / Math.max(1, axisEnd - axisStart))); }
    function available(time) { return backend.seekable && time >= seekStart && time <= seekEnd; }
    function seek(time) { if (available(time)) backend.seek_to(time); }
    function previewLabel(time) { return timeLabel(time); }
    readonly property real seekEnd: Number.isFinite(backend.window_end_ms)
        ? backend.window_end_ms : backend.duration_ms
    readonly property alias navigationSlider: slider
    property Item upNavigation: null
    property Item downNavigation: null
    signal adjustmentFinished
    property bool closing: false
    readonly property bool pressed: slider.pressed
    readonly property bool hovered: slider.enabled && seekHover.hovered
    spacing: 0

    function timeLabel(milliseconds) {
        if (milliseconds < 0 || !Number.isFinite(milliseconds)) return "--:--";
        const total = Math.floor(milliseconds / 1000);
        const hours = Math.floor(total / 3600);
        const minutes = Math.floor(total / 60) % 60;
        const seconds = total % 60;
        return (hours > 0 ? hours + ":" + String(minutes).padStart(2, "0") : String(minutes))
            + ":" + String(seconds).padStart(2, "0");
    }

    RowLayout {
        Layout.fillWidth: true
        Label {
            objectName: "recordingTime"
            text: root.timeLabel(slider.pressed ? slider.value : root.seekTarget >= 0 ? root.seekTarget : root.backend.position_ms)
                + " / " + (root.backend.duration_estimated === true ? "≈" : "") + root.timeLabel(root.backend.duration_ms)

            color: Theme.textPrimary
            font.pixelSize: Theme.fontCaption
            font.family: "monospace"
        }
        Label {
            Layout.fillWidth: true
            horizontalAlignment: Text.AlignRight
            text: root.backend.transport_error || (root.backend.seeking ? qsTranslate("Viewer", "Seeking…")
                : root.backend.ended ? qsTranslate("Backend", "Playback finished")
                : root.backend.paused ? qsTranslate("Backend", "Paused") : "")
            textFormat: Text.PlainText
            elide: Text.ElideRight
            color: root.backend.transport_error ? Theme.error : Theme.textSecondary
            font.pixelSize: Theme.fontCaption
        }
    }
    SeekSlider {
        id: slider
        KeyNavigation.priority: KeyNavigation.BeforeItem
        KeyNavigation.up: root.upNavigation
        KeyNavigation.down: root.downNavigation
        Keys.onLeftPressed: {
            if (root.backend.skip(slider.mirrored ? root.seekSteps.timelineMilliseconds : -root.seekSteps.timelineMilliseconds)) slider.flashSeek();
        }
        Keys.onRightPressed: {
            if (root.backend.skip(slider.mirrored ? -root.seekSteps.timelineMilliseconds : root.seekSteps.timelineMilliseconds)) slider.flashSeek();
        }
        Keys.onReturnPressed: function(event) { if (!event.isAutoRepeat) root.adjustmentFinished(); }
        Keys.onEnterPressed: function(event) { if (!event.isAutoRepeat) root.adjustmentFinished(); }
        objectName: "recordingSeekSlider"
        Layout.fillWidth: true
        enabled: !root.closing && root.backend.seekable && root.seekEnd > root.seekStart
        Accessible.name: qsTranslate("Viewer", "Playback position")
        from: root.axisStart
        to: root.axisEnd
        stepSize: root.millisecondsPerSecond
        indicatedValue: root.seekTarget >= 0 ? root.seekTarget : value
        positionText: root.timeLabel(indicatedValue)
        background: Item {
            id: track
            x: slider.leftPadding + slider.handle.width / 2
            y: slider.topPadding + slider.availableHeight / 2 - height / 2
            width: slider.availableWidth - slider.handle.width
            height: slider.retainedThickness
            function startX(start, end) { return (slider.mirrored ? 1 - root.fraction(end) : root.fraction(start)) * width; }
            Rectangle {
                objectName: "programTrack"
                anchors.verticalCenter: parent.verticalCenter
                anchors.alignWhenCentered: false
                width: parent.width; height: slider.trackThickness; radius: height / 2; color: Theme.overlayBorder
                Behavior on height { NumberAnimation { duration: Theme.colorDuration } }
            }
            Rectangle {
                objectName: "programProgressFill"
                x: parent.startX(root.progressStart, root.progressEnd)
                width: Math.max(0, root.fraction(root.progressEnd) - root.fraction(root.progressStart)) * parent.width
                anchors.verticalCenter: parent.verticalCenter
                anchors.alignWhenCentered: false
                height: slider.progressThickness; radius: height / 2; color: Theme.textPrimary
                Behavior on height { NumberAnimation { duration: Theme.colorDuration } }
            }
            Rectangle {
                objectName: "recordingSeekTargetMarker"
                visible: root.seekTarget >= 0
                x: (slider.mirrored ? 1 - root.fraction(root.seekTarget) : root.fraction(root.seekTarget)) * parent.width - width / 2
                anchors.verticalCenter: parent.verticalCenter
                anchors.alignWhenCentered: false
                width: 14; height: width; radius: width / 2
                color: "transparent"; border.width: 2; border.color: Theme.accent
            }
        }
        property var pendingTarget: null
        onEnabledChanged: if (!enabled) pendingTarget = null
        onMoved: {
            pendingTarget = value;
            if (!pressed) {
                root.seek(pendingTarget);
                pendingTarget = null;
            }
        }
        onPressedChanged: {
            if (!pressed && pendingTarget !== null) {
                if (enabled) root.seek(pendingTarget);
                pendingTarget = null;
            }
        }
        Binding {
            target: slider
            property: "value"
            value: Math.max(slider.from, Math.min(slider.to, root.progressEnd))
            when: !slider.pressed
            restoreMode: Binding.RestoreNone
        }
        HoverHandler { id: seekHover }
        SeekPreview {
            id: preview
            objectName: "recordingSeekPreview"
            parent: slider
            // Match the thumb's travel, including padding and its half-width.
            readonly property real fraction: Math.max(0, Math.min(1,
                (seekHover.point.position.x - slider.leftPadding - slider.handle.width / 2)
                / Math.max(1, slider.availableWidth - slider.handle.width)))
            readonly property real target: slider.from + (slider.to - slider.from)
                * (slider.mirrored ? 1 - fraction : fraction)
            visible: root.visible && !root.closing && slider.enabled && !slider.positionLabelVisible && (root.hovered || root.pressed)
            positionMs: slider.pressed ? slider.value : target
            imageSource: root.backend.seek_preview_image || ""
            onRequested: milliseconds => root.backend.request_seek_preview(milliseconds)
            onDismissed: root.backend.clear_seek_preview()
            text: root.previewLabel(slider.pressed ? slider.value : target)
            x: Math.max(0, Math.min(slider.width - implicitWidth,
                seekHover.point.position.x - implicitWidth / 2))
            y: -implicitHeight - 6
            padding: Theme.spaceSm
            font.family: "monospace"
        }
    }
}
