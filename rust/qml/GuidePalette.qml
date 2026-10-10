pragma Singleton
import QtQuick

// Broadcast genre and time-band colors carry information. Each genre keeps its
// conventional hue as a saturated marker over a dark tint of the same hue, so
// the guide shares the application's dark surfaces without losing the genre.
QtObject {
    readonly property var timeBands: ["#172340", "#15303b", "#153a31", "#2e3816", "#3a3016", "#3b2517", "#391731", "#2e183b"]
    readonly property var genres: ["#2a291b", "#1f2135", "#2e1f28", "#2f2020", "#1c2b20", "#1a2a2b", "#2e261c", "#2b1f2f", "#2a291b", "#2e261c", "#1c2632", "#1c2632"]
    readonly property var genreMarkers: ["#d9c76a", "#9299ea", "#e58fb8", "#e98a8a", "#7fcf8f", "#6fcfd0", "#e3a96c", "#d98ae0", "#d9c76a", "#e3a96c", "#7fb4e6", "#7fb4e6"]
    readonly property color defaultGenre: "#1f221f"
    readonly property color defaultMarker: "#8c918c"
    readonly property color programText: "#f4f5f3"
    readonly property color programSecondary: "#b6bab6"
    readonly property color cellDivider: "#0b0c0b"
    readonly property color logoBackground: "#ffffff"
    // Programs on air stand out from the schedule; finished ones recede.
    readonly property real onAirLightness: 1.5
    readonly property real hoverLightness: 1.25
    readonly property real finishedOpacity: 0.6
    readonly property int markerWidth: 3
}
