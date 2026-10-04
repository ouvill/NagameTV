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
    // Receiver color keys retain their broadcast identity in every UI language.
    readonly property color remoteBlue: "#639cf4"
    readonly property color remoteRed: "#ef7777"
    readonly property color remoteGreen: "#75c68b"
    readonly property color remoteYellow: "#e5cc64"

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
    readonly property real focusScale: 1.05
    readonly property int focusMaxGrowth: 8 // Cap the growth of wide text buttons.
    readonly property int focusGap: 2
    readonly property int focusWidth: 2
    readonly property int focusOutset: focusGap + focusWidth
    readonly property int focusDuration: 120
    readonly property int pressDuration: 65
    readonly property int colorDuration: 100
    readonly property int fadeInDuration: 120
    readonly property int fadeOutDuration: 100
    readonly property int moveDuration: 160
    readonly property int panelDuration: 240
    readonly property int seekFeedbackDuration: 1000
    readonly property int seekThumbSize: 12
    readonly property int seekThumbActiveSize: 18
    readonly property int seekFocusGap: focusGap
    readonly property int seekFocusWidth: focusWidth
    readonly property int seekTrackHeight: 4
    readonly property int seekTrackActiveHeight: 6
    readonly property int seekProgressHeight: 3
    readonly property int seekProgressActiveHeight: 5
    readonly property int seekRetainedHeight: 8
    readonly property int seekRetainedActiveHeight: 10
}
