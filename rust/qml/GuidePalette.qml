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
    // The accent almost vanishes on the mint and cyan genre cells, so the cursor
    // cell lifts out of the grid instead: it grows, rounds and casts a shadow.
    // Why not an outline: a frame on one pastel cell looked dated; a dark fill looked foreign.
    readonly property int cursorLift: 8
    readonly property color cursorShadow: "#d0000000"
    // The lifted cell also deepens its own genre color; an accent tint would hide the genre.
    readonly property real cursorDeepen: 0.1
    // Reservation marks sit on the pastel cells and on dark detail panels alike.
    readonly property color reservedMark: "#c2334d"
    readonly property color recordingMark: "#d11f2f"
    readonly property color reservationProblemMark: "#8a5a00"
    readonly property color markText: "#ffffff"
}
