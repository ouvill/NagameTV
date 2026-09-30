import QtQml

// Shared by playback buttons, keyboard shortcuts and timeline navigation.
QtObject {
    readonly property int backwardSeconds: 10
    readonly property int forwardSeconds: 30
    readonly property int timelineSeconds: 10
    readonly property int millisecondsPerSecond: 1000
    readonly property int backwardMilliseconds: -backwardSeconds * millisecondsPerSecond
    readonly property int forwardMilliseconds: forwardSeconds * millisecondsPerSecond
    readonly property int timelineMilliseconds: timelineSeconds * millisecondsPerSecond
}
