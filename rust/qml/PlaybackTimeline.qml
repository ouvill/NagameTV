pragma ComponentBehavior: Bound
import QtQuick

Loader {
    id: root
    required property var backend
    property bool closing: false
    readonly property RecordingTimeline recordingItem: item as RecordingTimeline
    readonly property LiveTimeline liveItem: item as LiveTimeline
    readonly property bool pressed: recordingItem ? recordingItem.pressed : liveItem ? liveItem.pressed : false
    readonly property bool hovered: recordingItem ? recordingItem.hovered : liveItem ? liveItem.hovered : false
    sourceComponent: backend.recording ? recording : live
    Component {
        id: recording
        RecordingTimeline { backend: root.backend; closing: root.closing }
    }
    Component {
        id: live
        LiveTimeline { backend: root.backend; closing: root.closing }
    }
}
