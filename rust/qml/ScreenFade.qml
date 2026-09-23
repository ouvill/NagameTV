import QtQuick
import MinimalViewer

NumberAnimation {
    property bool entering: true
    readonly property int enterDurationMs: Theme.fadeInDuration
    readonly property int exitDurationMs: Theme.fadeOutDuration
    property: "opacity"
    duration: entering ? enterDurationMs : exitDurationMs
    easing.type: Easing.Linear
}
