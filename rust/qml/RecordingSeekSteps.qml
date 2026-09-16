import QtQml

// Shared by playback buttons and keyboard shortcuts.
QtObject {
    readonly property int backwardSeconds: 10
    readonly property int forwardSeconds: 30
    readonly property int millisecondsPerSecond: 1000
    readonly property int backwardMilliseconds: -backwardSeconds * millisecondsPerSecond
    readonly property int forwardMilliseconds: forwardSeconds * millisecondsPerSecond
}
