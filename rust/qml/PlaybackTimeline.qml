pragma ComponentBehavior: Bound
import QtQuick

Loader {
    id: root
    required property var backend
    property bool closing: false
    readonly property bool pressed: item ? item.pressed : false
    readonly property bool hovered: item ? item.hovered : false
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
