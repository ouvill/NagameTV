pragma Singleton
import QtQuick

// Presentation values only. Application state belongs to the Rust use cases.
QtObject {
    readonly property color canvas: "#07090c"
    readonly property color surface: "#0f1217"
    readonly property color surfaceRaised: "#181c24"
    readonly property color surfaceHover: "#232936"
    readonly property color surfacePressed: "#1c3d37"
    readonly property color surfaceSelected: "#152f2b"
    readonly property color textPrimary: "#f4f6f8"
    readonly property color textSecondary: "#b4bbc7"
    readonly property color textMuted: "#848c9a"
    readonly property color textOnAccent: "#04231b"
    readonly property color accent: "#5ee0b9"
    readonly property color accentHover: "#8aedd0"
    readonly property color accentPressed: "#44c8a1"
    readonly property color selection: "#385ee0b9"
    readonly property color border: "#2b313c"
    readonly property color divider: "#232933"
    readonly property color warning: "#ffb86b"
    readonly property color warningSurface: "#261f15"
    readonly property color error: "#ff9d94"
    readonly property color destructive: "#c8453c"
    readonly property color live: "#ff5964"
    readonly property color progressTrack: "#33ffffff"
    // The stopped viewer's backdrop: a faint accent tint that fades into the canvas.
    readonly property color ambientGlow: "#0f2526"
    // Receiver color keys retain their broadcast identity in every UI language.
    readonly property color remoteBlue: "#639cf4"
    readonly property color remoteRed: "#ef7777"
    readonly property color remoteGreen: "#75c68b"
    readonly property color remoteYellow: "#e5cc64"

    // Translucent surfaces retain contrast over video, independent of its colors.
    readonly property color overlaySurface: "#e60c0f14"
    readonly property color popupSurface: "#f5101319"
    readonly property color overlayHover: "#26ffffff"
    readonly property color overlayPressed: "#585ee0b9"
    readonly property color overlayBorder: "#2effffff"
    readonly property color scrim: "#b3000000"
    readonly property color videoShade: "#d6000000"
    readonly property color videoShadeMiddle: "#52000000"
    readonly property color videoShadeStart: "#06000000"
    readonly property color textShadow: "#90000000"
    readonly property color track: "#945a6372"
    readonly property color trackSelection: "#7af4f6f8"
    readonly property color switchTrack: "#3d4451"

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
    readonly property real watchedOpacity: 0.6 // A finished recording recedes but stays legible.
    readonly property real pressScale: 0.97
    readonly property real cardPressScale: 0.985 // Large cards move less than buttons.
    readonly property real cardFocusScale: 1.03
    readonly property int progressHeight: 4
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
