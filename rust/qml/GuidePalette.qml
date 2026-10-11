pragma Singleton
import QtQuick

// Broadcast genre and time-band colors carry information, independently of
// the application's dark control palette. Keep text contrast with these cells.
QtObject {
    readonly property var timeBands: ["#00337f", "#00667f", "#007f66", "#667f00", "#7f6600", "#7f3300", "#7f0066", "#66007f"]
    readonly property var genres: ["#ffffe0", "#e0e0ff", "#ffe0f0", "#ffe0e0", "#e0ffe0", "#e0ffff", "#fff0e0", "#ffe0ff", "#ffffe0", "#fff0e0", "#e0f0ff", "#e0f0ff"]
    readonly property color defaultGenre: "#f0f0f0"
    readonly property color programText: "#252a31"
    readonly property color programSecondary: "#4e5651"
    readonly property color cellDivider: "#e4e7e4"
    readonly property color logoBackground: "#ffffff"
}
