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
    // The accent outline almost vanishes on the mint and cyan genre cells, so
    // the cursor cell turns dark and the outline sits on the dark fill instead.
    readonly property color cursorFill: "#181c24"
    readonly property color cursorText: "#f4f6f8"
    readonly property color cursorSecondary: "#b4bbc7"
    readonly property int cursorOutline: 3
    readonly property int cursorGenreStripe: 6
    // Reservation marks sit on the pastel cells and on dark detail panels alike.
    readonly property color reservedMark: "#c2334d"
    readonly property color recordingMark: "#d11f2f"
    readonly property color reservationProblemMark: "#8a5a00"
    readonly property color markText: "#ffffff"
}
