import QtQuick

NumberAnimation {
    property bool entering: true
    readonly property int enterDurationMs: 120
    readonly property int exitDurationMs: 100
    property: "opacity"
    duration: entering ? enterDurationMs : exitDurationMs
    easing.type: Easing.Linear
}
