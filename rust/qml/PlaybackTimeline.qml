pragma ComponentBehavior: Bound
import QtQuick
import MinimalViewer

Loader {
    id: root
    required property var backend
    property bool closing: false
    property Item upNavigation: null
    property Item downNavigation: null
    signal adjustmentFinished
    readonly property SeekSlider navigationSlider: recordingItem ? recordingItem.navigationSlider : liveItem ? liveItem.navigationSlider : null
    function flashSeek() {
        if (navigationSlider) navigationSlider.flashSeek();
    }
    function enter(): bool {
        if (!navigationSlider || !navigationSlider.enabled || !navigationSlider.visible) return false;
        navigationSlider.forceActiveFocus(Qt.TabFocusReason);
        return true;
    }
    readonly property RecordingTimeline recordingItem: item as RecordingTimeline
    readonly property LiveTimeline liveItem: item as LiveTimeline
    readonly property bool pressed: recordingItem ? recordingItem.pressed : liveItem ? liveItem.pressed : false
    readonly property bool hovered: recordingItem ? recordingItem.hovered : liveItem ? liveItem.hovered : false
    sourceComponent: backend.recording ? recording : live
    Component {
        id: recording
        RecordingTimeline {
            backend: root.backend; closing: root.closing
            upNavigation: root.upNavigation; downNavigation: root.downNavigation
            onAdjustmentFinished: root.adjustmentFinished()
        }
    }
    Component {
        id: live
        LiveTimeline {
            backend: root.backend; closing: root.closing
            upNavigation: root.upNavigation; downNavigation: root.downNavigation
            onAdjustmentFinished: root.adjustmentFinished()
        }
    }
}
