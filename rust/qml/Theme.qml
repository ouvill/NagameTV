pragma Singleton
import QtQuick

// Presentation values only. Application state belongs to the Rust use cases.
QtObject {
    readonly property color canvas: "#0b0c0b"
    readonly property color surface: "#151715"
    readonly property color surfaceRaised: "#222622"
    readonly property color surfaceHover: "#2a302b"
    readonly property color surfacePressed: "#344238"
    readonly property color surfaceSelected: "#26302a"
    readonly property color textPrimary: "#f4f5f3"
    readonly property color textSecondary: "#b6bab6"
    readonly property color textMuted: "#8c918c"
    readonly property color textOnAccent: "#17201a"
    readonly property color accent: "#9caf9f"
    readonly property color accentHover: "#b6c6b8"
    readonly property color accentPressed: "#8da793"
    readonly property color selection: "#389caf9f"
    readonly property color border: "#3b423c"
    readonly property color divider: "#343c35"
    readonly property color warning: "#ffb080"
    readonly property color warningSurface: "#24211d"
    readonly property color error: "#ffb4ab"
    readonly property color destructive: "#a94b3f"
    readonly property color live: "#e36b6b"

    // Translucent surfaces retain contrast over video, independent of its colors.
    readonly property color overlaySurface: "#e6171819"
    readonly property color popupSurface: "#f21a1c1a"
    readonly property color overlayHover: "#28ffffff"
    readonly property color overlayPressed: "#589caf9f"
    readonly property color overlayBorder: "#38ffffff"
    readonly property color scrim: "#a8000000"
    readonly property color videoShade: "#d6000000"
    readonly property color videoShadeMiddle: "#52000000"
    readonly property color videoShadeStart: "#06000000"
    readonly property color textShadow: "#90000000"
    readonly property color track: "#9468716b"
    readonly property color trackSelection: "#7af4f5f3"
    readonly property color switchTrack: "#4c4f4c"

    readonly property int fontMicro: 10 // Dense timeline and icon annotations only.
    readonly property int fontCaption: 12
    readonly property int fontBody: 14
    readonly property int fontControl: 16
    readonly property int fontHeading: 18
    readonly property int fontTitle: 24
    readonly property int fontDisplay: 30
    readonly property int spaceXs: 4
    readonly property int spaceSm: 8
    readonly property int spaceMd: 12
    readonly property int spaceLg: 16
    readonly property int spaceXl: 24
    readonly property int controlHeight: 44
    readonly property int compactControlHeight: 32
    readonly property int iconButtonSize: 42
    readonly property int iconSize: 24
    readonly property int smallIconSize: 18
    readonly property int controlRadius: 8
    readonly property int panelRadius: 12
    readonly property int indicatorRadius: 2
    readonly property real disabledOpacity: 0.42
    readonly property real pressScale: 0.97
    readonly property real cardPressScale: 0.985 // Large cards move less than buttons.
    readonly property int pressDuration: 65
    readonly property int colorDuration: 100
    readonly property int fadeInDuration: 120
    readonly property int fadeOutDuration: 100
    readonly property int moveDuration: 160
    readonly property int panelDuration: 240
}
